use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Weak},
};

use chrono::{DateTime, Utc};
use ring::hmac;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    PgConnection, PgPool, Row,
};
use tokio::sync::{Mutex, OwnedMutexGuard, OwnedSemaphorePermit, RwLock, Semaphore};
use uuid::Uuid;

use crate::{
    auth::{AuthRateLimit, StoredRecoveryCode, StoredSession, StoredUser},
    config::Config,
    error::ApiError,
    models::{
        AnalyticsConsentReceipt, AnalyticsEventReceipt, AuditEvent, BackgroundOperation,
        ContactRequest, ContentEntry, GeneralInformation, GuestVisit, NewsEntry, PlatformSettings,
        Product, ProductImportResult, ProductPresentation, RfqSubmission, SourceSnapshot,
        StagingRecord, StagingValidationStatus, SyncConflict, SyncRun, SyncRunStatus,
        TemporaryOverride, UpdatePlatformSettings, ValidationIssue,
    },
    rate_limit::InMemoryRateLimit,
    services::product_facts::project_product_facts,
    services::product_import::verify_product_master_authority,
    services::product_publication::{
        immutable_revision_payload, issues_as_errors, validate_accepted_staging_payload,
        validate_product_master, validate_source_owned_alignment,
        validate_verified_csv_product_master, workflow_error,
    },
};

#[cfg(feature = "devtools")]
#[derive(Clone, Debug)]
pub(crate) struct DevtoolTokenGrant {
    pub expires_at: std::time::Instant,
    pub session_id: Uuid,
    pub actor: String,
    pub user_id: Uuid,
    pub admin_session_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct IdempotencyRecord {
    pub request_hash: String,
    pub response: Value,
    pub response_status: u16,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct IdempotencyReplay {
    pub response: Value,
    pub response_status: u16,
}

pub(crate) struct IdempotencyGuard {
    _memory_guard: OwnedMutexGuard<()>,
    database_transaction: Option<sqlx::Transaction<'static, sqlx::Postgres>>,
    _database_slot: Option<OwnedSemaphorePermit>,
}

impl IdempotencyGuard {
    pub(crate) async fn finish(mut self) -> Result<(), ApiError> {
        if let Some(transaction) = self.database_transaction.take() {
            transaction.commit().await?;
        }
        Ok(())
    }
}

const IDEMPOTENCY_TTL_HOURS: i64 = 24;
const MAX_IN_MEMORY_IDEMPOTENCY_KEYS: usize = 10_000;

fn derive_analytics_storage_id(external_id: Uuid, secret: &[u8; 32]) -> Uuid {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let digest = hmac::sign(&key, external_id.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_ref()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Default)]
pub struct PlatformData {
    pub settings: PlatformSettings,
    pub content: HashMap<Uuid, ContentEntry>,
    pub content_revisions: HashMap<Uuid, BTreeMap<i64, ContentEntry>>,
    pub published_content: HashMap<Uuid, ContentEntry>,
    pub news: HashMap<Uuid, NewsEntry>,
    pub published_news: HashMap<Uuid, NewsEntry>,
    pub general_information: HashMap<Uuid, GeneralInformation>,
    pub general_information_revisions: HashMap<Uuid, BTreeMap<i64, GeneralInformation>>,
    pub published_general_information: HashMap<Uuid, GeneralInformation>,
    pub products: HashMap<Uuid, Product>,
    pub product_revisions: HashMap<Uuid, BTreeMap<i64, Product>>,
    pub product_presentations: HashMap<(Uuid, String), ProductPresentation>,
    pub published_products: HashMap<Uuid, Product>,
    pub source_snapshots: HashMap<Uuid, SourceSnapshot>,
    pub staging_records: HashMap<Uuid, StagingRecord>,
    pub sync_runs: HashMap<Uuid, SyncRun>,
    pub conflicts: HashMap<Uuid, SyncConflict>,
    pub temporary_overrides: HashMap<Uuid, TemporaryOverride>,
    pub rfqs: HashMap<Uuid, RfqSubmission>,
    pub contacts: HashMap<Uuid, ContactRequest>,
    pub analytics_consents: HashMap<Uuid, AnalyticsConsentReceipt>,
    pub analytics_receipts: HashMap<Uuid, AnalyticsEventReceipt>,
    pub guest_visits: HashMap<Uuid, GuestVisit>,
    pub product_imports: HashMap<Uuid, ProductImportResult>,
    pub operations: HashMap<Uuid, BackgroundOperation>,
    pub audit_events: Vec<AuditEvent>,
    pub idempotency: HashMap<(String, String), IdempotencyRecord>,
    pub admin_users: HashMap<Uuid, StoredUser>,
    pub admin_sessions: HashMap<Uuid, StoredSession>,
    pub recovery_codes: HashMap<Uuid, Vec<StoredRecoveryCode>>,
    pub auth_rate_limits: HashMap<String, AuthRateLimit>,
    pub public_rate_limits: HashMap<(String, String), InMemoryRateLimit>,
    pub outbox_events: Vec<Value>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: Option<PgPool>,
    pub data: Arc<RwLock<PlatformData>>,
    pub auth_hash_slots: Arc<Semaphore>,
    idempotency_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
    idempotency_database_slots: Arc<Semaphore>,
    #[cfg(feature = "devtools")]
    pub(crate) devtool_tokens: Arc<RwLock<HashMap<String, DevtoolTokenGrant>>>,
    #[cfg(feature = "devtools")]
    pub(crate) devtool_session_slots: Arc<Semaphore>,
}

impl AppState {
    /// Derive the database identifier used for a browser-provided anonymous
    /// session. PostgreSQL never stores the client UUID verbatim. The
    /// in-memory adapter intentionally keeps the original ID for isolated unit
    /// tests that do not have deployment secrets.
    pub(crate) fn analytics_storage_session_id(&self, external_id: Uuid) -> Result<Uuid, ApiError> {
        if self.pool.is_none() {
            return Ok(external_id);
        }
        let secret = self
            .config
            .analytics_token_hmac_key
            .as_ref()
            .ok_or_else(|| {
                ApiError::service_unavailable(
                    "Analytics HMAC storage key is not configured for PostgreSQL.",
                )
            })?;
        // RFC 9562 UUIDv8 with the RFC variant. This is only a compact database
        // representation of a keyed digest, not a public capability.
        Ok(derive_analytics_storage_id(external_id, secret.as_bytes()))
    }

    pub fn new(config: Config) -> Result<Self, ApiError> {
        if config.production && config.database_url.is_none() {
            return Err(ApiError::service_unavailable(
                "PostgreSQL persistence is required in production.",
            ));
        }
        let pool = match &config.database_url {
            Some(database_url) => Some(
                PgPoolOptions::new()
                    .max_connections(10)
                    .connect_lazy(database_url)
                    .map_err(|_| ApiError::bad_request("DATABASE_URL is invalid."))?,
            ),
            None => None,
        };

        Ok(Self {
            config: Arc::new(config),
            pool,
            data: Arc::new(RwLock::new(PlatformData::default())),
            auth_hash_slots: Arc::new(Semaphore::new(4)),
            idempotency_locks: Arc::new(Mutex::new(HashMap::new())),
            // A guard holds one PostgreSQL transaction while the business
            // mutation uses other pool connections. Keep ample pool headroom.
            idempotency_database_slots: Arc::new(Semaphore::new(4)),
            #[cfg(feature = "devtools")]
            devtool_tokens: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "devtools")]
            devtool_session_slots: Arc::new(Semaphore::new(4)),
        })
    }

    pub fn for_test() -> Self {
        Self::new(Config::for_test()).expect("test configuration is valid")
    }

    pub fn environment_label(&self) -> &'static str {
        if self.config.production {
            "production"
        } else {
            "development"
        }
    }

    pub async fn check_persistence(&self) -> Result<String, ApiError> {
        match &self.pool {
            Some(pool) => {
                let ready = sqlx::query_scalar::<_, bool>(
                    "SELECT to_regclass('public.content_entries') IS NOT NULL",
                )
                .fetch_one(pool)
                .await?;
                if !ready {
                    return Err(ApiError::service_unavailable(
                        "PostgreSQL is reachable but platform migrations have not been applied.",
                    ));
                }
                Ok("postgresql".into())
            }
            None if self.config.production => Err(ApiError::service_unavailable(
                "PostgreSQL persistence is required in production.",
            )),
            None => Ok("inMemory".into()),
        }
    }

    pub async fn integer_setting(
        &self,
        key: &str,
        default: i64,
        minimum: i64,
        maximum: i64,
    ) -> Result<i64, ApiError> {
        let Some(pool) = &self.pool else {
            let settings = &self.data.read().await.settings;
            let value = match key {
                "rfqRetentionDays" => settings.rfq_retention_days,
                "retentionDeletionGraceDays" => settings.retention_deletion_grace_days,
                "temporaryOverrideDefaultDays" => settings.temporary_override_default_days,
                _ => default,
            };
            return value
                .clamp(minimum, maximum)
                .eq(&value)
                .then_some(value)
                .ok_or_else(|| {
                    ApiError::service_unavailable(format!(
                        "The `{key}` setting must be an integer between {minimum} and {maximum}."
                    ))
                });
        };
        let value = sqlx::query_scalar::<_, Value>("SELECT value FROM app_settings WHERE key=$1")
            .bind(key)
            .fetch_optional(pool)
            .await?
            .unwrap_or(Value::from(default));
        value
            .as_i64()
            .filter(|value| (minimum..=maximum).contains(value))
            .ok_or_else(|| {
                ApiError::service_unavailable(format!(
                    "The `{key}` setting must be an integer between {minimum} and {maximum}."
                ))
            })
    }

    pub async fn platform_settings(&self) -> Result<PlatformSettings, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.settings.clone());
        };
        let row = sqlx::query(
            r#"SELECT state.revision,
                      (SELECT value FROM app_settings WHERE key='rfqRetentionDays') AS rfq_retention_days,
                      (SELECT value FROM app_settings WHERE key='retentionDeletionGraceDays') AS retention_deletion_grace_days,
                      (SELECT value FROM app_settings WHERE key='temporaryOverrideDefaultDays') AS temporary_override_default_days,
                      (SELECT value FROM app_settings WHERE key='publicLocale') AS public_locale
               FROM platform_settings_state state
               WHERE state.singleton=true"#,
        )
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            ApiError::service_unavailable("The platform settings revision row is missing.")
        })?;
        decode_platform_settings_row(&row)
    }

    pub async fn update_platform_settings(
        &self,
        expected_revision: i64,
        update: &UpdatePlatformSettings,
        actor: &str,
        request_id: Uuid,
    ) -> Result<PlatformSettings, ApiError> {
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            let row = sqlx::query(
                r#"SELECT state.revision,
                          (SELECT value FROM app_settings WHERE key='rfqRetentionDays') AS rfq_retention_days,
                          (SELECT value FROM app_settings WHERE key='retentionDeletionGraceDays') AS retention_deletion_grace_days,
                          (SELECT value FROM app_settings WHERE key='temporaryOverrideDefaultDays') AS temporary_override_default_days,
                          (SELECT value FROM app_settings WHERE key='publicLocale') AS public_locale
                   FROM platform_settings_state state
                   WHERE state.singleton=true
                   FOR UPDATE OF state"#,
            )
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| {
                ApiError::service_unavailable("The platform settings revision row is missing.")
            })?;
            let before = decode_platform_settings_row(&row)?;
            if before.revision != expected_revision {
                return Err(ApiError::conflict(
                    "The platform settings changed; reload before saving.",
                ));
            }
            let after = apply_platform_settings_update(&before, update)?;
            let now = Utc::now();

            for (key, value) in [
                ("rfqRetentionDays", Value::from(after.rfq_retention_days)),
                (
                    "retentionDeletionGraceDays",
                    Value::from(after.retention_deletion_grace_days),
                ),
                (
                    "temporaryOverrideDefaultDays",
                    Value::from(after.temporary_override_default_days),
                ),
            ] {
                sqlx::query(
                    r#"UPDATE app_settings
                       SET value=$1, updated_at=$2, updated_by=$3
                       WHERE key=$4"#,
                )
                .bind(value)
                .bind(now)
                .bind(actor)
                .bind(key)
                .execute(&mut *transaction)
                .await?;
            }
            let updated = sqlx::query(
                r#"UPDATE platform_settings_state
                   SET revision=$1, updated_at=$2, updated_by=$3
                   WHERE singleton=true AND revision=$4"#,
            )
            .bind(after.revision)
            .bind(now)
            .bind(actor)
            .bind(expected_revision)
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "The platform settings changed; reload before saving.",
                ));
            }

            let event = settings_audit_event(&before, &after, update, actor, request_id, now);
            insert_audit_in_transaction(&mut transaction, &event).await?;
            transaction.commit().await?;
            return Ok(after);
        }

        let mut data = self.data.write().await;
        let before = data.settings.clone();
        if before.revision != expected_revision {
            return Err(ApiError::conflict(
                "The platform settings changed; reload before saving.",
            ));
        }
        let after = apply_platform_settings_update(&before, update)?;
        let event = settings_audit_event(&before, &after, update, actor, request_id, Utc::now());
        data.settings = after.clone();
        data.audit_events.push(event);
        Ok(after)
    }

    pub async fn assert_product_publishable(&self, product: &Product) -> Result<(), ApiError> {
        let issues = if self.pool.is_some() {
            self.postgres_product_publication_issues(product).await?
        } else {
            let mut issues = validate_product_master(product);
            issues.extend(self.in_memory_product_publication_issues(product).await);
            issues
        };
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ApiError::validation(issues_as_errors(issues)))
        }
    }

    /// Presentation publication has its own revision clock. A Product Master
    /// fact revision can remain current while an editor has a newer website
    /// draft that still needs to be projected publicly.
    pub async fn product_presentation_is_published(
        &self,
        product_id: Uuid,
        locale: &str,
    ) -> Result<bool, ApiError> {
        if let Some(pool) = &self.pool {
            return Ok(sqlx::query_scalar::<_, bool>(
                r#"SELECT published_revision=current_revision
                   FROM product_presentation_working
                   WHERE product_id=$1 AND locale=$2"#,
            )
            .bind(product_id)
            .bind(locale)
            .fetch_optional(pool)
            .await?
            .unwrap_or(false));
        }
        Ok(self
            .data
            .read()
            .await
            .product_presentations
            .get(&(product_id, locale.to_owned()))
            .is_some_and(|presentation| {
                presentation.published_revision == Some(presentation.revision)
            }))
    }

    async fn in_memory_product_publication_issues(
        &self,
        product: &Product,
    ) -> Vec<ValidationIssue> {
        let data = self.data.read().await;
        let mut issues = Vec::new();
        let immutable_revision_matches = data
            .product_revisions
            .get(&product.id)
            .and_then(|revisions| revisions.get(&product.current_revision))
            .is_some_and(|revision| {
                revision.source_snapshot_id == product.source_snapshot_id
                    && immutable_revision_payload(revision).ok()
                        == immutable_revision_payload(product).ok()
            });
        if !immutable_revision_matches {
            issues.push(workflow_error(
                "currentRevision",
                "immutableRevisionMismatch",
                "The current immutable Product revision is missing or differs from the working record.",
            ));
        }
        let now = Utc::now();
        let active_override_paths: Vec<_> = data
            .temporary_overrides
            .values()
            .filter(|value| value.product_id == product.id && value.expires_at > now)
            .map(|value| value.field_path.clone())
            .collect();
        let snapshot = data.source_snapshots.get(&product.source_snapshot_id);
        let source_record_id = if let Some(snapshot) = snapshot {
            if snapshot.connector_id.is_nil()
                || snapshot.sync_run_id.is_nil()
                || snapshot.checksum.trim().is_empty()
            {
                issues.push(workflow_error(
                    "sourceSnapshotId",
                    "invalidSourceSnapshot",
                    "The source snapshot is not bound to a Feishu connector, sync run and checksum.",
                ));
            }
            if snapshot.source_revision.trim() != product.source_revision.trim() {
                issues.push(workflow_error(
                    "sourceRevision",
                    "sourceRevisionMismatch",
                    "The working Product source revision does not match its source snapshot.",
                ));
            }
            Some(snapshot.source_record_id.as_str())
        } else {
            issues.push(workflow_error(
                "sourceSnapshotId",
                "sourceSnapshotNotFound",
                "The referenced Feishu source snapshot does not exist.",
            ));
            None
        };

        let staging = snapshot.and_then(|snapshot| {
            data.staging_records
                .values()
                .filter(|record| {
                    record.source_snapshot_id == snapshot.id
                        && record.sync_run_id == snapshot.sync_run_id
                        && record.source_record_id == snapshot.source_record_id
                })
                .max_by_key(|record| record.created_at)
        });
        if let Some(staging) = staging {
            if staging.validation_status != StagingValidationStatus::Valid
                || !staging.validation_errors.is_empty()
            {
                issues.push(workflow_error(
                    "staging.validationStatus",
                    "stagingNotAccepted",
                    "The exact source snapshot has not passed staging validation without errors.",
                ));
            }
            match &staging.normalized_payload {
                Some(payload) if !payload.is_null() => {
                    issues.extend(validate_accepted_staging_payload(product, payload));
                    issues.extend(validate_source_owned_alignment(
                        product,
                        payload,
                        &active_override_paths,
                    ));
                }
                _ => issues.push(workflow_error(
                    "staging.normalizedPayload",
                    "normalizedPayloadRequired",
                    "Accepted staging evidence requires a normalized Product Master payload.",
                )),
            }
            match data.sync_runs.get(&staging.sync_run_id) {
                Some(run)
                    if run.source == "feishu"
                        && !run.dry_run
                        && matches!(
                            run.status,
                            SyncRunStatus::ReadyToPublish | SyncRunStatus::Completed
                        ) => {}
                _ => issues.push(workflow_error(
                    "staging.syncRunId",
                    "syncRunNotAccepted",
                    "Publishing requires a non-dry-run Feishu sync in readyToPublish or completed state.",
                )),
            }
        } else {
            issues.push(workflow_error(
                "staging.validationStatus",
                "stagingRecordNotFound",
                "No accepted staging record exists for the exact source snapshot.",
            ));
        }

        if data.conflicts.values().any(|conflict| {
            conflict.resolved_at.is_none()
                && (conflict.product_id == Some(product.id)
                    || source_record_id.is_some_and(|source_record_id| {
                        conflict.source_record_id == source_record_id
                    }))
        }) {
            issues.push(workflow_error(
                "conflicts",
                "openConflict",
                "All open three-way conflicts related to this Product must be resolved.",
            ));
        }
        if data
            .temporary_overrides
            .values()
            .any(|value| value.product_id == product.id && value.expires_at <= now)
        {
            issues.push(workflow_error(
                "temporaryOverrides",
                "expiredOverride",
                "An unresolved expired temporary override blocks publication.",
            ));
        }
        issues
    }

    async fn postgres_product_publication_issues(
        &self,
        product: &Product,
    ) -> Result<Vec<ValidationIssue>, ApiError> {
        let pool = self.pool.as_ref().expect("checked by caller");
        let mut connection = pool.acquire().await?;
        self.postgres_product_publication_issues_on(&mut connection, product)
            .await
    }

    async fn postgres_product_publication_issues_on(
        &self,
        connection: &mut PgConnection,
        product: &Product,
    ) -> Result<Vec<ValidationIssue>, ApiError> {
        let mut issues = Vec::new();
        let working_row = sqlx::query(
            r#"SELECT product.payload AS working_payload,
                      product.source_snapshot_id AS working_source_snapshot_id,
                      product.source_revision AS working_source_revision,
                      product.data_origin AS working_data_origin,
                      product.product_import_run_id AS working_import_run_id,
                      product.current_revision AS working_revision,
                      revision.payload AS immutable_payload,
                      revision.source_snapshot_id AS immutable_source_snapshot_id,
                      revision.data_origin AS immutable_data_origin,
                      revision.product_import_run_id AS immutable_import_run_id
               FROM products AS product
               LEFT JOIN product_revisions AS revision
                 ON revision.product_id=product.id
                AND revision.revision=product.current_revision
               WHERE product.id=$1"#,
        )
        .bind(product.id)
        .fetch_optional(&mut *connection)
        .await?;
        let mut data_origin = String::new();
        let mut import_run_id = None;
        if let Some(row) = working_row {
            let working_payload: Value = row.try_get("working_payload")?;
            let stored_working: Product = decode_payload(working_payload, "working product")?;
            let immutable_payload: Option<Value> = row.try_get("immutable_payload")?;
            let immutable_source_snapshot_id: Option<Uuid> =
                row.try_get("immutable_source_snapshot_id")?;
            let working_source_snapshot_id: Option<Uuid> =
                row.try_get("working_source_snapshot_id")?;
            data_origin = row.try_get("working_data_origin")?;
            import_run_id = row.try_get("working_import_run_id")?;
            let immutable_data_origin: String = row.try_get("immutable_data_origin")?;
            let immutable_import_run_id: Option<Uuid> = row.try_get("immutable_import_run_id")?;
            let expected_snapshot =
                (!product.source_snapshot_id.is_nil()).then_some(product.source_snapshot_id);
            let immutable_matches = immutable_payload
                .and_then(|payload| decode_payload::<Product>(payload, "product revision").ok())
                .is_some_and(|revision| {
                    immutable_source_snapshot_id == expected_snapshot
                        && immutable_revision_payload(&revision).ok()
                            == immutable_revision_payload(product).ok()
                });
            if row.try_get::<i64, _>("working_revision")? != product.current_revision
                || working_source_snapshot_id != expected_snapshot
                || row.try_get::<String, _>("working_source_revision")?.trim()
                    != product.source_revision.trim()
                || immutable_data_origin != data_origin
                || immutable_import_run_id != import_run_id
                || immutable_revision_payload(&stored_working).ok()
                    != immutable_revision_payload(product).ok()
                || !immutable_matches
            {
                issues.push(workflow_error(
                    "currentRevision",
                    "immutableRevisionMismatch",
                    "The stored working Product and current immutable revision must match the requested source revision.",
                ));
            }
            if data_origin == "verifiedCsv" {
                issues.extend(validate_verified_csv_product_master(&stored_working));
            } else {
                issues.extend(validate_product_master(&stored_working));
            }
        } else {
            issues.push(workflow_error(
                "currentRevision",
                "immutableRevisionMissing",
                "The stored working Product and current immutable revision are required.",
            ));
        }

        let override_rows = sqlx::query(
            r#"SELECT field_path, expires_at <= now() AS expired
               FROM product_temporary_overrides
               WHERE product_id=$1 AND resolved_at IS NULL"#,
        )
        .bind(product.id)
        .fetch_all(&mut *connection)
        .await?;
        let mut active_override_paths = Vec::new();
        let mut expired_override = false;
        for row in override_rows {
            if row.try_get::<bool, _>("expired")? {
                expired_override = true;
            } else {
                active_override_paths.push(row.try_get::<String, _>("field_path")?);
            }
        }
        if expired_override {
            issues.push(workflow_error(
                "temporaryOverrides",
                "expiredOverride",
                "An unresolved expired temporary override blocks publication.",
            ));
        }

        if data_origin == "verifiedCsv" {
            let evidence = sqlx::query(
                r#"SELECT run.environment,run.status,run.source_checksum,run.mapping_version,
                          run.records_received,run.records_valid,
                          record.validation_status,record.normalized_payload
                   FROM product_import_runs run
                   LEFT JOIN product_import_normalized_records record
                     ON record.import_run_id=run.id AND record.source_record_id=$2
                   WHERE run.id=$1 AND run.data_origin='verifiedCsv'"#,
            )
            .bind(import_run_id)
            .bind(&product.stable_id)
            .fetch_optional(&mut *connection)
            .await?;
            match evidence {
                Some(row) => {
                    let status: String = row.try_get("status")?;
                    let environment: String = row.try_get("environment")?;
                    let checksum: String = row.try_get("source_checksum")?;
                    let mapping_version: String = row.try_get("mapping_version")?;
                    let records_received: i64 = row.try_get("records_received")?;
                    let records_valid: i64 = row.try_get("records_valid")?;
                    let validation_status: Option<String> = row.try_get("validation_status")?;
                    let normalized: Option<Value> = row.try_get("normalized_payload")?;
                    if status != "completed"
                        || mapping_version.trim().is_empty()
                        || validation_status.as_deref() != Some("valid")
                        || product.source_revision != format!("csv:{checksum}")
                    {
                        issues.push(workflow_error(
                            "productImportRunId",
                            "verifiedCsvEvidenceInvalid",
                            "Verified CSV publication requires a completed matching import run and valid normalized record.",
                        ));
                    }
                    if environment != self.environment_label()
                        || verify_product_master_authority(
                            self.environment_label(),
                            self.config.approved_product_master.as_ref(),
                            &checksum,
                            &mapping_version,
                            records_valid,
                            records_received.saturating_sub(records_valid),
                        )
                        .is_err()
                    {
                        issues.push(workflow_error(
                            "productImportRunId",
                            "verifiedCsvAuthorityMismatch",
                            "The Product Master import does not match this deployment's registered source, mapping, environment, and reviewed row counts.",
                        ));
                    }
                    if let Some(normalized) = normalized {
                        issues.extend(validate_accepted_staging_payload(product, &normalized));
                        issues.extend(validate_source_owned_alignment(
                            product,
                            &normalized,
                            &active_override_paths,
                        ));
                    } else {
                        issues.push(workflow_error(
                            "productImportRunId",
                            "verifiedCsvRecordMissing",
                            "The normalized verified CSV record is missing.",
                        ));
                    }
                }
                None => issues.push(workflow_error(
                    "productImportRunId",
                    "verifiedCsvImportMissing",
                    "The verified CSV import run is missing.",
                )),
            }
            return Ok(issues);
        }

        let row = sqlx::query(
            r#"SELECT snapshot.connector_id,
                      connector.connector_type,
                      snapshot.sync_run_id AS snapshot_sync_run_id,
                      snapshot.source_record_id AS snapshot_source_record_id,
                      snapshot.source_revision AS snapshot_source_revision,
                      snapshot.checksum AS snapshot_checksum,
                      staging.source_record_id AS staging_source_record_id,
                      staging.validation_status,
                      staging.normalized_payload,
                      staging.validation_errors,
                      run.source AS sync_source,
                      run.dry_run AS sync_dry_run,
                      run.status AS sync_status,
                      run.mapping_version AS sync_mapping_version
               FROM source_snapshots AS snapshot
               LEFT JOIN source_connectors AS connector ON connector.id=snapshot.connector_id
               LEFT JOIN staging_records AS staging
                 ON staging.source_snapshot_id=snapshot.id
                AND staging.sync_run_id=snapshot.sync_run_id
                AND staging.source_record_id=snapshot.source_record_id
               LEFT JOIN sync_runs AS run ON run.id=staging.sync_run_id
               WHERE snapshot.id=$1
               ORDER BY staging.created_at DESC
               LIMIT 1"#,
        )
        .bind(product.source_snapshot_id)
        .fetch_optional(&mut *connection)
        .await?;

        let mut source_record_id = None;
        if let Some(row) = row {
            let connector_id: Option<Uuid> = row.try_get("connector_id")?;
            let connector_type: Option<String> = row.try_get("connector_type")?;
            let snapshot_sync_run_id: Option<Uuid> = row.try_get("snapshot_sync_run_id")?;
            let snapshot_source_record_id: String = row.try_get("snapshot_source_record_id")?;
            let snapshot_source_revision: String = row.try_get("snapshot_source_revision")?;
            let snapshot_checksum: String = row.try_get("snapshot_checksum")?;
            source_record_id = Some(snapshot_source_record_id.clone());
            if connector_id.is_none()
                || connector_type.as_deref() != Some("feishu")
                || snapshot_sync_run_id.is_none()
                || snapshot_checksum.trim().is_empty()
            {
                issues.push(workflow_error(
                    "sourceSnapshotId",
                    "invalidSourceSnapshot",
                    "The source snapshot is not bound to a Feishu connector, sync run and checksum.",
                ));
            }
            if snapshot_source_revision.trim() != product.source_revision.trim() {
                issues.push(workflow_error(
                    "sourceRevision",
                    "sourceRevisionMismatch",
                    "The working Product source revision does not match its source snapshot.",
                ));
            }

            let staging_source_record_id: Option<String> =
                row.try_get("staging_source_record_id")?;
            let validation_status: Option<String> = row.try_get("validation_status")?;
            let validation_errors: Option<Value> = row.try_get("validation_errors")?;
            if staging_source_record_id.as_deref() != Some(snapshot_source_record_id.as_str())
                || validation_status.as_deref() != Some("valid")
                || !validation_errors
                    .as_ref()
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
            {
                issues.push(workflow_error(
                    "staging.validationStatus",
                    "stagingNotAccepted",
                    "The exact source snapshot has not passed staging validation without errors.",
                ));
            }
            match row.try_get::<Option<Value>, _>("normalized_payload")? {
                Some(payload) if !payload.is_null() => {
                    issues.extend(validate_accepted_staging_payload(product, &payload));
                    issues.extend(validate_source_owned_alignment(
                        product,
                        &payload,
                        &active_override_paths,
                    ));
                }
                _ => issues.push(workflow_error(
                    "staging.normalizedPayload",
                    "normalizedPayloadRequired",
                    "Accepted staging evidence requires a normalized Product Master payload.",
                )),
            }
            let sync_source: Option<String> = row.try_get("sync_source")?;
            let sync_dry_run: Option<bool> = row.try_get("sync_dry_run")?;
            let sync_status: Option<String> = row.try_get("sync_status")?;
            let sync_mapping_version: Option<String> = row.try_get("sync_mapping_version")?;
            if sync_source.as_deref() != Some("feishu")
                || sync_dry_run != Some(false)
                || sync_mapping_version
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(|value| value.is_empty())
                || !matches!(sync_status.as_deref(), Some("readyToPublish" | "completed"))
            {
                issues.push(workflow_error(
                    "staging.syncRunId",
                    "syncRunNotAccepted",
                    "Publishing requires a non-dry-run Feishu sync in readyToPublish or completed state.",
                ));
            }
        } else {
            issues.push(workflow_error(
                "sourceSnapshotId",
                "sourceSnapshotNotFound",
                "The referenced Feishu source snapshot does not exist.",
            ));
            issues.push(workflow_error(
                "staging.validationStatus",
                "stagingRecordNotFound",
                "No accepted staging record exists for the exact source snapshot.",
            ));
        }

        let open_conflict = sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(
                   SELECT 1 FROM sync_conflicts
                   WHERE resolved_at IS NULL
                     AND (product_id=$1 OR ($2::text IS NOT NULL AND source_record_id=$2))
               )"#,
        )
        .bind(product.id)
        .bind(source_record_id)
        .fetch_one(&mut *connection)
        .await?;
        if open_conflict {
            issues.push(workflow_error(
                "conflicts",
                "openConflict",
                "All open three-way conflicts related to this Product must be resolved.",
            ));
        }

        Ok(issues)
    }

    /// Verify the database without copying runtime records into PlatformData.
    /// Business entities and settings are deliberately never snapshotted
    /// when PostgreSQL is configured: request-time SQL is the production source
    /// of truth, so independent API instances observe publishing immediately.
    pub async fn hydrate(&self) -> Result<(), ApiError> {
        if self.pool.is_none() {
            return Ok(());
        }
        self.check_persistence().await?;
        Ok(())
    }

    // Kept temporarily as a private migration aid while the remaining legacy
    // handlers are converted to request-time SQL. It is never called by the
    // application and therefore cannot become a production business cache.
    #[allow(dead_code)]
    async fn hydrate_legacy_snapshot(&self) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        self.check_persistence().await?;
        let mut loaded = PlatformData {
            settings: self.platform_settings().await?,
            ..PlatformData::default()
        };

        for row in sqlx::query("SELECT payload FROM content_entries")
            .fetch_all(pool)
            .await?
        {
            let entry: ContentEntry = decode_payload(row.try_get("payload")?, "content")?;
            loaded.content.insert(entry.id, entry);
        }
        for row in sqlx::query("SELECT content_id, revision, payload FROM content_revisions")
            .fetch_all(pool)
            .await?
        {
            let content_id: Uuid = row.try_get("content_id")?;
            let revision: i64 = row.try_get("revision")?;
            let entry: ContentEntry = decode_payload(row.try_get("payload")?, "content revision")?;
            loaded
                .content_revisions
                .entry(content_id)
                .or_default()
                .insert(revision, entry);
        }
        for working in loaded.content.values() {
            let Some(revision) = working.published_revision else {
                continue;
            };
            if let Some(snapshot) = loaded
                .content_revisions
                .get(&working.id)
                .and_then(|revisions| revisions.get(&revision))
            {
                let mut published = snapshot.clone();
                published.status = crate::models::PublicationStatus::Published;
                published.published_revision = Some(revision);
                if published.is_placeholder {
                    published.seo.indexable = false;
                }
                loaded.published_content.insert(working.id, published);
            }
        }
        for row in sqlx::query(
            r#"SELECT id, connector_id, sync_run_id, source_record_id,
                      source_revision, checksum, source_payload, received_at
               FROM source_snapshots
               WHERE connector_id IS NOT NULL AND sync_run_id IS NOT NULL"#,
        )
        .fetch_all(pool)
        .await?
        {
            let snapshot = SourceSnapshot {
                id: row.try_get("id")?,
                connector_id: row.try_get("connector_id")?,
                sync_run_id: row.try_get("sync_run_id")?,
                source_record_id: row.try_get("source_record_id")?,
                source_revision: row.try_get("source_revision")?,
                checksum: row.try_get("checksum")?,
                source_payload: row.try_get("source_payload")?,
                received_at: row.try_get("received_at")?,
            };
            loaded.source_snapshots.insert(snapshot.id, snapshot);
        }
        for row in sqlx::query(
            r#"SELECT id, sync_run_id, source_snapshot_id, source_record_id,
                      validation_status, normalized_payload, validation_errors, created_at
               FROM staging_records"#,
        )
        .fetch_all(pool)
        .await?
        {
            let record = StagingRecord {
                id: row.try_get("id")?,
                sync_run_id: row.try_get("sync_run_id")?,
                source_snapshot_id: row.try_get("source_snapshot_id")?,
                source_record_id: row.try_get("source_record_id")?,
                validation_status: decode_enum(
                    row.try_get("validation_status")?,
                    "staging validation status",
                )?,
                normalized_payload: row.try_get("normalized_payload")?,
                validation_errors: decode_payload(
                    row.try_get("validation_errors")?,
                    "staging validation errors",
                )?,
                created_at: row.try_get("created_at")?,
            };
            loaded.staging_records.insert(record.id, record);
        }
        for row in sqlx::query("SELECT payload FROM products")
            .fetch_all(pool)
            .await?
        {
            let product: Product = decode_payload(row.try_get("payload")?, "product")?;
            loaded.products.insert(product.id, product);
        }
        for row in sqlx::query("SELECT product_id, revision, payload FROM product_revisions")
            .fetch_all(pool)
            .await?
        {
            let product_id: Uuid = row.try_get("product_id")?;
            let revision: i64 = row.try_get("revision")?;
            let product: Product = decode_payload(row.try_get("payload")?, "product revision")?;
            loaded
                .product_revisions
                .entry(product_id)
                .or_default()
                .insert(revision, product);
        }
        for row in sqlx::query(
            r#"SELECT product_id,locale,current_revision,published_revision,slug,title,
                      summary,content,seo_metadata,indexable,updated_at
               FROM product_presentation_working"#,
        )
        .fetch_all(pool)
        .await?
        {
            let product_id: Uuid = row.try_get("product_id")?;
            let locale: String = row.try_get("locale")?;
            let content: Value = row.try_get("content")?;
            let sort_order = content
                .get("sortOrder")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or_default();
            let related_content_ids = content
                .get("relatedContentIds")
                .cloned()
                .map(|value| decode_payload(value, "product related content ids"))
                .transpose()?
                .unwrap_or_default();
            loaded.product_presentations.insert(
                (product_id, locale.clone()),
                ProductPresentation {
                    locale,
                    slug: row.try_get("slug")?,
                    title: row.try_get("title")?,
                    summary: row.try_get("summary")?,
                    seo: decode_payload(row.try_get("seo_metadata")?, "product presentation SEO")?,
                    indexable: row.try_get("indexable")?,
                    sort_order,
                    related_content_ids,
                    revision: row.try_get("current_revision")?,
                    published_revision: row.try_get("published_revision")?,
                    updated_at: row.try_get("updated_at")?,
                },
            );
        }
        for working in loaded.products.values() {
            let Some(revision) = working.published_revision else {
                continue;
            };
            if let Some(snapshot) = loaded
                .product_revisions
                .get(&working.id)
                .and_then(|revisions| revisions.get(&revision))
            {
                let mut published = snapshot.clone();
                published.status = crate::models::PublicationStatus::Published;
                published.published_revision = Some(revision);
                loaded.published_products.insert(working.id, published);
            }
        }
        for row in sqlx::query("SELECT payload FROM sync_runs")
            .fetch_all(pool)
            .await?
        {
            let run: SyncRun = decode_payload(row.try_get("payload")?, "sync run")?;
            loaded.sync_runs.insert(run.id, run);
        }
        for row in sqlx::query("SELECT payload FROM rfq_submissions")
            .fetch_all(pool)
            .await?
        {
            let submission: RfqSubmission = decode_payload(row.try_get("payload")?, "RFQ")?;
            loaded.rfqs.insert(submission.id, submission);
        }
        for row in sqlx::query("SELECT payload FROM contact_requests")
            .fetch_all(pool)
            .await?
        {
            let contact: ContactRequest =
                decode_payload(row.try_get("payload")?, "contact request")?;
            loaded.contacts.insert(contact.id, contact);
        }
        for row in sqlx::query(
            "SELECT id, product_id, field_path, value, reason, created_at, expires_at FROM product_temporary_overrides WHERE resolved_at IS NULL",
        )
        .fetch_all(pool)
        .await?
        {
            let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
            let value = TemporaryOverride {
                id: row.try_get("id")?,
                product_id: row.try_get("product_id")?,
                field_path: row.try_get("field_path")?,
                value: row.try_get("value")?,
                reason: row.try_get("reason")?,
                created_at: row.try_get("created_at")?,
                expires_at,
                expired: expires_at <= Utc::now(),
            };
            loaded.temporary_overrides.insert(value.id, value);
        }
        for row in sqlx::query(
            "SELECT id, sync_run_id, product_id, source_record_id, field_diffs, resolved_at, resolution FROM sync_conflicts",
        )
        .fetch_all(pool)
        .await?
        {
            let conflict = SyncConflict {
                id: row.try_get("id")?,
                sync_run_id: row.try_get("sync_run_id")?,
                product_id: row.try_get("product_id")?,
                source_record_id: row.try_get("source_record_id")?,
                diffs: decode_payload(row.try_get("field_diffs")?, "sync conflict")?,
                resolved_at: row.try_get("resolved_at")?,
                resolution: row.try_get("resolution")?,
            };
            loaded.conflicts.insert(conflict.id, conflict);
        }
        for row in sqlx::query(
            "SELECT id, kind, status, reason, result, created_at, updated_at FROM operation_runs",
        )
        .fetch_all(pool)
        .await?
        {
            let operation = BackgroundOperation {
                id: row.try_get("id")?,
                kind: decode_enum(row.try_get("kind")?, "operation kind")?,
                status: decode_enum(row.try_get("status")?, "operation status")?,
                reason: row.try_get("reason")?,
                result: row.try_get("result")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            loaded.operations.insert(operation.id, operation);
        }
        for row in sqlx::query(
            "SELECT id, actor, action, entity_type, entity_id, before_value, after_value, reason, request_id, occurred_at FROM audit_log ORDER BY occurred_at DESC LIMIT 1000",
        )
        .fetch_all(pool)
        .await?
        {
            loaded.audit_events.push(AuditEvent {
                id: row.try_get("id")?,
                actor: row.try_get("actor")?,
                action: row.try_get("action")?,
                entity_type: row.try_get("entity_type")?,
                entity_id: row.try_get("entity_id")?,
                before: row.try_get("before_value")?,
                after: row.try_get("after_value")?,
                reason: row.try_get("reason")?,
                request_id: row.try_get("request_id")?,
                occurred_at: row.try_get("occurred_at")?,
            });
        }

        *self.data.write().await = loaded;
        Ok(())
    }

    pub async fn idempotency_replay(
        &self,
        scope: &str,
        key: &str,
        request_hash: &str,
    ) -> Result<Option<IdempotencyReplay>, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                "SELECT request_hash, response_body, response_status FROM idempotency_keys WHERE scope=$1 AND key_hash=$2 AND expires_at > now()",
            )
            .bind(scope)
            .bind(text_hash(key))
            .fetch_optional(pool)
            .await?;
            let Some(row) = row else {
                return Ok(None);
            };
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(ApiError::conflict(
                    "This Idempotency-Key was already used with a different request body.",
                ));
            }
            let status: i32 = row.try_get("response_status")?;
            let response_status = u16::try_from(status).map_err(|_| {
                ApiError::service_unavailable("Stored idempotency response status is invalid.")
            })?;
            return Ok(Some(IdempotencyReplay {
                response: row.try_get("response_body")?,
                response_status,
            }));
        }

        let mut data = self.data.write().await;
        let record_key = (scope.into(), key.into());
        if data.idempotency.get(&record_key).is_some_and(|stored| {
            stored.created_at + chrono::Duration::hours(IDEMPOTENCY_TTL_HOURS) <= Utc::now()
        }) {
            data.idempotency.remove(&record_key);
        }
        let Some(stored) = data.idempotency.get(&record_key) else {
            return Ok(None);
        };
        if stored.request_hash != request_hash {
            return Err(ApiError::conflict(
                "This Idempotency-Key was already used with a different request body.",
            ));
        }
        Ok(Some(IdempotencyReplay {
            response: stored.response.clone(),
            response_status: stored.response_status,
        }))
    }

    pub(crate) async fn acquire_idempotency_guard(
        &self,
        scope: &str,
        key: &str,
    ) -> Result<IdempotencyGuard, ApiError> {
        let lock_key = text_hash(&format!("{scope}|{key}"));
        let lock = {
            let mut locks = self.idempotency_locks.lock().await;
            locks.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = locks.get(&lock_key).and_then(Weak::upgrade) {
                lock
            } else {
                if locks.len() >= MAX_IN_MEMORY_IDEMPOTENCY_KEYS {
                    return Err(ApiError::too_many_requests(
                        "Too many idempotent mutations are currently in progress.",
                    ));
                }
                let lock = Arc::new(Mutex::new(()));
                locks.insert(lock_key.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        let memory_guard = lock.lock_owned().await;
        let database_slot = if self.pool.is_some() {
            Some(
                self.idempotency_database_slots
                    .clone()
                    .acquire_owned()
                    .await
                    .map_err(|_| {
                        ApiError::service_unavailable("Idempotency coordination stopped.")
                    })?,
            )
        } else {
            None
        };
        let database_transaction = if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(&lock_key)
                .execute(&mut *transaction)
                .await?;
            Some(transaction)
        } else {
            None
        };
        Ok(IdempotencyGuard {
            _memory_guard: memory_guard,
            database_transaction,
            _database_slot: database_slot,
        })
    }

    pub async fn save_idempotency(
        &self,
        scope: &str,
        key: String,
        request_hash: String,
        response: Value,
        response_status: u16,
    ) -> Result<(), ApiError> {
        if let Some(pool) = &self.pool {
            let result = sqlx::query(
                r#"INSERT INTO idempotency_keys
                   (scope, key_hash, request_hash, response_status, response_body)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (scope, key_hash) DO UPDATE SET
                   request_hash=EXCLUDED.request_hash,
                   response_status=EXCLUDED.response_status,
                   response_body=EXCLUDED.response_body,
                   created_at=now(), expires_at=now() + interval '24 hours'
                   WHERE idempotency_keys.expires_at <= now()"#,
            )
            .bind(scope)
            .bind(text_hash(&key))
            .bind(&request_hash)
            .bind(response_status as i32)
            .bind(&response)
            .execute(pool)
            .await?;
            if result.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "This Idempotency-Key is already active for another request.",
                ));
            }
            return Ok(());
        }
        let mut data = self.data.write().await;
        let now = Utc::now();
        data.idempotency.retain(|_, record| {
            record.created_at + chrono::Duration::hours(IDEMPOTENCY_TTL_HOURS) > now
        });
        if !data.idempotency.contains_key(&(scope.into(), key.clone()))
            && data.idempotency.len() >= MAX_IN_MEMORY_IDEMPOTENCY_KEYS
        {
            return Err(ApiError::too_many_requests(
                "The in-memory idempotency store is at capacity.",
            ));
        }
        data.idempotency.insert(
            (scope.into(), key),
            IdempotencyRecord {
                request_hash,
                response,
                response_status,
                created_at: Utc::now(),
            },
        );
        Ok(())
    }

    pub async fn list_working_content(&self) -> Result<Vec<ContentEntry>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.content.values().cloned().collect());
        };
        let rows = sqlx::query("SELECT payload FROM content_entries ORDER BY updated_at DESC,id")
            .fetch_all(pool)
            .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "content"))
            .collect()
    }

    pub async fn load_working_content(&self, id: Uuid) -> Result<Option<ContentEntry>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.content.get(&id).cloned());
        };
        sqlx::query_scalar::<_, Value>("SELECT payload FROM content_entries WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(|payload| decode_payload(payload, "content"))
            .transpose()
    }

    pub async fn content_identity_exists(
        &self,
        kind: crate::models::ContentKind,
        slug: &str,
        locale: &str,
        exclude_id: Option<Uuid>,
    ) -> Result<bool, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.content.values().any(|entry| {
                Some(entry.id) != exclude_id
                    && entry.kind == kind
                    && entry.slug == slug
                    && entry.locale == locale
            }));
        };
        Ok(sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(SELECT 1 FROM content_entries
               WHERE kind=$1 AND slug=$2 AND locale=$3
                 AND ($4::uuid IS NULL OR id<>$4))"#,
        )
        .bind(enum_label(kind))
        .bind(slug)
        .bind(locale)
        .bind(exclude_id)
        .fetch_one(pool)
        .await?)
    }

    pub async fn list_working_products(&self) -> Result<Vec<Product>, ApiError> {
        let Some(pool) = &self.pool else {
            let data = self.data.read().await;
            return Ok(data
                .products
                .values()
                .cloned()
                .map(|mut product| {
                    if let Some(presentation) = data
                        .product_presentations
                        .get(&(product.id, product.locale.clone()))
                    {
                        overlay_presentation(&mut product, presentation);
                    }
                    product
                })
                .collect());
        };
        let rows = sqlx::query(
            r#"SELECT product.payload,
                      presentation.current_revision AS presentation_revision,
                      presentation.locale AS presentation_locale,
                      presentation.slug AS presentation_slug,
                      presentation.title AS presentation_title,
                      presentation.summary AS presentation_summary,
                      presentation.content AS presentation_content,
                      presentation.seo_metadata AS presentation_seo,
                      presentation.indexable AS presentation_indexable
               FROM products product
               LEFT JOIN product_presentation_working presentation
                 ON presentation.product_id=product.id
                AND presentation.locale=product.locale
               ORDER BY product.stable_id,product.id"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let mut product = decode_payload(row.try_get("payload")?, "product")?;
                overlay_presentation_row(&mut product, &row)?;
                Ok(product)
            })
            .collect()
    }

    pub async fn load_working_product(&self, id: Uuid) -> Result<Option<Product>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.products.get(&id).cloned());
        };
        sqlx::query_scalar::<_, Value>("SELECT payload FROM products WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(|payload| decode_payload(payload, "product"))
            .transpose()
    }

    /// Combine Product Master facts with the current portal-owned presentation
    /// for an Admin response without ever writing the combined DTO back into
    /// the immutable fact payload.
    pub async fn present_product(&self, mut product: Product) -> Result<Product, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                r#"SELECT current_revision AS presentation_revision,
                          locale AS presentation_locale,slug AS presentation_slug,
                          title AS presentation_title,summary AS presentation_summary,
                          content AS presentation_content,seo_metadata AS presentation_seo,
                          indexable AS presentation_indexable
                   FROM product_presentation_working
                   WHERE product_id=$1 AND locale=$2"#,
            )
            .bind(product.id)
            .bind(&product.locale)
            .fetch_optional(pool)
            .await?;
            if let Some(row) = row {
                overlay_presentation_row(&mut product, &row)?;
            }
            return Ok(product);
        }
        if let Some(presentation) = self
            .data
            .read()
            .await
            .product_presentations
            .get(&(product.id, product.locale.clone()))
        {
            overlay_presentation(&mut product, presentation);
        }
        Ok(product)
    }

    pub async fn list_stored_rfqs(&self) -> Result<Vec<RfqSubmission>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.rfqs.values().cloned().collect());
        };
        let rows = sqlx::query("SELECT payload FROM rfq_submissions ORDER BY submitted_at DESC,id")
            .fetch_all(pool)
            .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "RFQ"))
            .collect()
    }

    pub async fn list_stored_contacts(&self) -> Result<Vec<ContactRequest>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.contacts.values().cloned().collect());
        };
        let rows =
            sqlx::query("SELECT payload FROM contact_requests ORDER BY submitted_at DESC,id")
                .fetch_all(pool)
                .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "contact request"))
            .collect()
    }

    pub async fn list_stored_audit(&self) -> Result<Vec<AuditEvent>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.audit_events.clone());
        };
        let rows = sqlx::query(
            r#"SELECT id,actor,action,entity_type,entity_id,before_value,after_value,
                      reason,request_id,occurred_at
               FROM audit_log ORDER BY occurred_at DESC,id"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AuditEvent {
                    id: row.try_get("id")?,
                    actor: row.try_get("actor")?,
                    action: row.try_get("action")?,
                    entity_type: row.try_get("entity_type")?,
                    entity_id: row.try_get("entity_id")?,
                    before: row.try_get("before_value")?,
                    after: row.try_get("after_value")?,
                    reason: row.try_get("reason")?,
                    request_id: row.try_get("request_id")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect()
    }

    pub async fn persist_content(&self, entry: &ContentEntry, actor: &str) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content serialization failed."))?;
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                r#"INSERT INTO content_entries
               (id, kind, slug, locale, title, status, is_placeholder, current_revision,
                published_revision, scheduled_for, payload, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
               ON CONFLICT (id) DO UPDATE SET kind=EXCLUDED.kind, slug=EXCLUDED.slug,
               locale=EXCLUDED.locale, title=EXCLUDED.title, status=EXCLUDED.status,
               is_placeholder=EXCLUDED.is_placeholder,
               current_revision=EXCLUDED.current_revision,
               published_revision=EXCLUDED.published_revision,
               scheduled_for=EXCLUDED.scheduled_for, payload=EXCLUDED.payload,
               updated_at=EXCLUDED.updated_at"#,
            )
            .bind(entry.id)
            .bind(enum_label(entry.kind))
            .bind(&entry.slug)
            .bind(&entry.locale)
            .bind(&entry.title)
            .bind(enum_label(entry.status))
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(entry.published_revision)
            .bind(entry.scheduled_for)
            .bind(&payload)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            // Creation is an explicit snapshot boundary. Later autosaves only
            // update the working row; preview, publish, and rollback materialize
            // any additional immutable snapshots.
            sqlx::query(
                r#"INSERT INTO content_revisions
                   (content_id,revision,payload,created_by,created_at)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (content_id,revision) DO NOTHING"#,
            )
            .bind(entry.id)
            .bind(entry.current_revision)
            .bind(&payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        }
        if self.pool.is_none() {
            self.data
                .write()
                .await
                .content_revisions
                .entry(entry.id)
                .or_default()
                .entry(entry.current_revision)
                .or_insert_with(|| entry.clone());
        }
        Ok(())
    }

    /// Loads one immutable content snapshot. Preview callers must never fall
    /// back to the working record or the published projection when this exact
    /// revision does not exist.
    pub async fn load_content_revision(
        &self,
        content_id: Uuid,
        revision: i64,
    ) -> Result<Option<ContentEntry>, ApiError> {
        if let Some(pool) = &self.pool {
            let payload = sqlx::query_scalar::<_, Value>(
                "SELECT payload FROM content_revisions WHERE content_id=$1 AND revision=$2",
            )
            .bind(content_id)
            .bind(revision)
            .fetch_optional(pool)
            .await?;
            return payload
                .map(|payload| decode_payload(payload, "content preview revision"))
                .transpose();
        }

        Ok(self
            .data
            .read()
            .await
            .content_revisions
            .get(&content_id)
            .and_then(|revisions| revisions.get(&revision))
            .cloned())
    }

    /// Materialize an explicit preview snapshot of the current working
    /// document. Autosave itself does not create immutable history; preview,
    /// publish and rollback are the explicit snapshot boundaries.
    pub async fn snapshot_working_content(
        &self,
        entry: &ContentEntry,
        actor: &str,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content snapshot serialization failed."))?;
        if let Some(pool) = &self.pool {
            sqlx::query(
                r#"INSERT INTO content_revisions
                   (content_id,revision,payload,created_by,created_at)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (content_id,revision) DO NOTHING"#,
            )
            .bind(entry.id)
            .bind(entry.current_revision)
            .bind(payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(pool)
            .await?;
        }
        if self.pool.is_none() {
            self.data
                .write()
                .await
                .content_revisions
                .entry(entry.id)
                .or_default()
                .entry(entry.current_revision)
                .or_insert_with(|| entry.clone());
        }
        Ok(())
    }

    pub async fn persist_content_update(
        &self,
        entry: &ContentEntry,
        _actor: &str,
        expected_revision: i64,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content serialization failed."))?;
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            let result = sqlx::query(
                r#"UPDATE content_entries SET kind=$2, slug=$3, locale=$4, title=$5,
                   status=$6, is_placeholder=$7, current_revision=$8,
                   published_revision=$9, scheduled_for=$10, payload=$11, updated_at=$12,
                   data_origin=CASE
                     WHEN data_origin='developmentFixture' AND NOT $7 THEN 'editorial'
                     ELSE data_origin
                   END
                   WHERE id=$1 AND current_revision=$13"#,
            )
            .bind(entry.id)
            .bind(enum_label(entry.kind))
            .bind(&entry.slug)
            .bind(&entry.locale)
            .bind(&entry.title)
            .bind(enum_label(entry.status))
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(entry.published_revision)
            .bind(entry.scheduled_for)
            .bind(&payload)
            .bind(entry.updated_at)
            .bind(expected_revision)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The content entry changed; reload before saving.",
                ));
            }
            transaction.commit().await?;
        }
        Ok(())
    }

    /// Atomically moves the public pointer to an immutable revision and emits
    /// the invalidation/indexing event consumed by the worker.
    pub async fn publish_content_projection(
        &self,
        entry: &ContentEntry,
        actor: &str,
        event: &str,
        expected_revision: i64,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content serialization failed."))?;
        let outbox_id = Uuid::new_v4();
        let outbox_payload = serde_json::json!({
            "entityId": entry.id,
            "revision": entry.current_revision,
            "locale": entry.locale,
            "kind": enum_label(entry.kind),
            "event": event
        });
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            let result = sqlx::query(
                r#"UPDATE content_entries
                   SET kind=$2, slug=$3, locale=$4, title=$5, status='published',
                       is_placeholder=$6, current_revision=$7, published_revision=$7,
                       scheduled_for=NULL, payload=$8, updated_at=$9,
                       data_origin=CASE
                         WHEN data_origin='developmentFixture' AND NOT $6 THEN 'editorial'
                         ELSE data_origin
                       END
                   WHERE id=$1 AND current_revision=$10"#,
            )
            .bind(entry.id)
            .bind(enum_label(entry.kind))
            .bind(&entry.slug)
            .bind(&entry.locale)
            .bind(&entry.title)
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(&payload)
            .bind(entry.updated_at)
            .bind(expected_revision)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The content entry changed; reload before publishing.",
                ));
            }
            // A content entity owns at most one canonical route. Replacing or
            // removing the canonical path must retire every older locale/path
            // in the same publication transaction.
            sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
                .bind(entry.id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                r#"INSERT INTO content_revisions
                   (content_id, revision, payload, created_by, created_at)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (content_id, revision) DO NOTHING"#,
            )
            .bind(entry.id)
            .bind(entry.current_revision)
            .bind(&payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            if let Some(canonical_path) = entry.seo.canonical_path.as_deref() {
                let route_owner = sqlx::query_scalar::<_, Uuid>(
                    "SELECT entity_id FROM public_routes WHERE canonical_path=$1",
                )
                .bind(canonical_path)
                .fetch_optional(&mut *transaction)
                .await?;
                if route_owner.is_some_and(|owner| owner != entry.id) {
                    transaction.rollback().await?;
                    return Err(ApiError::conflict(
                        "Another published entity already owns this canonical path.",
                    ));
                }
                sqlx::query(
                    r#"INSERT INTO public_routes
                       (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
                       VALUES ($1,'content',$2,$3,$4,$5,$6)
                       ON CONFLICT (entity_type,entity_id,locale) DO UPDATE SET
                         canonical_path=EXCLUDED.canonical_path,
                         indexable=EXCLUDED.indexable,updated_at=EXCLUDED.updated_at"#,
                )
                .bind(Uuid::new_v4())
                .bind(entry.id)
                .bind(&entry.locale)
                .bind(canonical_path)
                .bind(entry.seo.indexable && !entry.is_placeholder)
                .bind(entry.updated_at)
                .execute(&mut *transaction)
                .await?;
            }
            sqlx::query(
                r#"INSERT INTO outbox_events
                   (id, topic, aggregate_type, aggregate_id, payload)
                   VALUES ($1,'public.content.published','content',$2,$3)
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(outbox_id)
            .bind(entry.id)
            .bind(&outbox_payload)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        }
        if self.pool.is_none() {
            let mut data = self.data.write().await;
            data.content_revisions
                .entry(entry.id)
                .or_default()
                .entry(entry.current_revision)
                .or_insert_with(|| entry.clone());
            data.published_content.insert(entry.id, entry.clone());
            if !data.outbox_events.iter().any(|value| {
                value.get("topic").and_then(Value::as_str) == Some("public.content.published")
                    && value.get("aggregateId") == Some(&serde_json::json!(entry.id))
                    && value.pointer("/payload/revision")
                        == Some(&serde_json::json!(entry.current_revision))
            }) {
                data.outbox_events.push(serde_json::json!({
                    "id": outbox_id,
                    "topic": "public.content.published",
                    "aggregateId": entry.id,
                    "payload": outbox_payload
                }));
            }
        }
        Ok(())
    }

    pub async fn persist_sync_run(&self, run: &SyncRun) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO sync_runs
               (id, source, dry_run, mapping_version, status, resume_cursor, records_seen,
                records_valid, conflict_count, started_at, completed_at, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
               ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status,
               resume_cursor=EXCLUDED.resume_cursor, records_seen=EXCLUDED.records_seen,
               records_valid=EXCLUDED.records_valid, conflict_count=EXCLUDED.conflict_count,
               completed_at=EXCLUDED.completed_at, payload=EXCLUDED.payload"#,
        )
        .bind(run.id)
        .bind(&run.source)
        .bind(run.dry_run)
        .bind(&run.mapping_version)
        .bind(enum_label(run.status))
        .bind(&run.resume_cursor)
        .bind(run.records_seen as i64)
        .bind(run.records_valid as i64)
        .bind(run.conflict_count as i64)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(
            serde_json::to_value(run)
                .map_err(|_| ApiError::internal("Sync serialization failed."))?,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn enqueue_sync_run(&self, run: &SyncRun) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let payload = serde_json::to_value(run)
            .map_err(|_| ApiError::internal("Sync serialization failed."))?;
        let job_payload = serde_json::json!({
            "syncRunId": run.id,
            "dryRun": run.dry_run,
            "mappingVersion": run.mapping_version,
            "cursor": run.resume_cursor,
        });
        let mut transaction = pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO sync_runs
               (id, source, dry_run, mapping_version, status, resume_cursor, records_seen,
                records_valid, conflict_count, started_at, completed_at, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        )
        .bind(run.id)
        .bind(&run.source)
        .bind(run.dry_run)
        .bind(&run.mapping_version)
        .bind(enum_label(run.status))
        .bind(&run.resume_cursor)
        .bind(run.records_seen as i64)
        .bind(run.records_valid as i64)
        .bind(run.conflict_count as i64)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(&payload)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO jobs
               (id, job_type, status, payload, available_at, created_at, updated_at)
               VALUES ($1,'feishuSync','queued',$2,$3,$3,$3)"#,
        )
        .bind(run.id)
        .bind(job_payload)
        .bind(run.started_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn list_sync_runs(&self) -> Result<Vec<SyncRun>, ApiError> {
        if let Some(pool) = &self.pool {
            let rows = sqlx::query("SELECT payload FROM sync_runs ORDER BY started_at DESC")
                .fetch_all(pool)
                .await?;
            return rows
                .into_iter()
                .map(|row| decode_payload(row.try_get("payload")?, "sync run"))
                .collect();
        }
        let mut values: Vec<_> = self.data.read().await.sync_runs.values().cloned().collect();
        values.sort_by_key(|run| std::cmp::Reverse(run.started_at));
        Ok(values)
    }

    pub async fn persist_product(&self, product: &Product) -> Result<(), ApiError> {
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("product:stable-id:{}", product.stable_id))
                .execute(&mut *transaction)
                .await?;
            let existing = sqlx::query(
                r#"SELECT id,current_revision,published_revision,data_origin,
                          source_snapshot_id,source_revision
                   FROM products WHERE stable_id=$1 FOR UPDATE"#,
            )
            .bind(&product.stable_id)
            .fetch_optional(&mut *transaction)
            .await?;
            let mut accepted = product.clone();
            // Feishu acceptance creates a working facts revision only. It can
            // never make that revision public or indexable without the
            // separate publication transaction and its evidence checks.
            accepted.status = crate::models::PublicationStatus::Draft;
            accepted.indexable = false;
            let mut insert_product = true;
            if let Some(existing) = existing {
                let data_origin: String = existing.try_get("data_origin")?;
                if data_origin == "developmentFixture" {
                    transaction.rollback().await?;
                    return Err(ApiError::conflict(
                        "A development fixture cannot be taken over as a Feishu Product.",
                    ));
                }
                let existing_id: Uuid = existing.try_get("id")?;
                let existing_revision: i64 = existing.try_get("current_revision")?;
                let existing_snapshot: Option<Uuid> = existing.try_get("source_snapshot_id")?;
                let existing_source_revision: String = existing.try_get("source_revision")?;
                let same_source = data_origin == "feishu"
                    && existing_snapshot == Some(product.source_snapshot_id)
                    && existing_source_revision == product.source_revision;
                accepted.id = existing_id;
                accepted.current_revision = if same_source {
                    existing_revision
                } else {
                    existing_revision + 1
                };
                accepted.published_revision = existing.try_get("published_revision")?;
                insert_product = false;
            }
            let mut payload = serde_json::to_value(&accepted)
                .map_err(|_| ApiError::internal("Product serialization failed."))?;
            if let Some(object) = payload.as_object_mut() {
                object.insert("dataOrigin".into(), Value::String("feishu".into()));
            }
            if insert_product {
                sqlx::query(
                    r#"INSERT INTO products
                       (id,stable_id,model,slug,locale,family,source_snapshot_id,
                        source_revision,status,current_revision,published_revision,indexable,
                        payload,updated_at,data_origin,product_import_run_id)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,
                               'feishu',NULL)"#,
                )
                .bind(accepted.id)
                .bind(&accepted.stable_id)
                .bind(&accepted.model)
                .bind(&accepted.slug)
                .bind(&accepted.locale)
                .bind(enum_label(accepted.family))
                .bind(accepted.source_snapshot_id)
                .bind(&accepted.source_revision)
                .bind(enum_label(accepted.status))
                .bind(accepted.current_revision)
                .bind(accepted.published_revision)
                .bind(accepted.indexable)
                .bind(&payload)
                .bind(accepted.updated_at)
                .execute(&mut *transaction)
                .await?;
            } else {
                sqlx::query(
                    r#"UPDATE products SET model=$2,slug=$3,locale=$4,family=$5,
                              source_snapshot_id=$6,source_revision=$7,status=$8,
                              current_revision=$9,published_revision=$10,indexable=$11,
                              payload=$12,updated_at=$13,data_origin='feishu',
                              product_import_run_id=NULL
                       WHERE id=$1"#,
                )
                .bind(accepted.id)
                .bind(&accepted.model)
                .bind(&accepted.slug)
                .bind(&accepted.locale)
                .bind(enum_label(accepted.family))
                .bind(accepted.source_snapshot_id)
                .bind(&accepted.source_revision)
                .bind(enum_label(accepted.status))
                .bind(accepted.current_revision)
                .bind(accepted.published_revision)
                .bind(accepted.indexable)
                .bind(&payload)
                .bind(accepted.updated_at)
                .execute(&mut *transaction)
                .await?;
            }
            sqlx::query(
                r#"INSERT INTO product_revisions
                   (product_id,revision,source_snapshot_id,payload,created_at,data_origin,
                    product_import_run_id)
                   VALUES ($1,$2,$3,$4,$5,'feishu',NULL)
                   ON CONFLICT (product_id,revision) DO NOTHING"#,
            )
            .bind(accepted.id)
            .bind(accepted.current_revision)
            .bind(accepted.source_snapshot_id)
            .bind(&payload)
            .bind(accepted.updated_at)
            .execute(&mut *transaction)
            .await?;
            let stored_revision = sqlx::query(
                r#"SELECT source_snapshot_id, payload
                   FROM product_revisions
                   WHERE product_id=$1 AND revision=$2"#,
            )
            .bind(accepted.id)
            .bind(accepted.current_revision)
            .fetch_one(&mut *transaction)
            .await?;
            let stored_source_snapshot_id: Uuid = stored_revision.try_get("source_snapshot_id")?;
            let stored_payload: Value = stored_revision.try_get("payload")?;
            let stored_product: Product = decode_payload(stored_payload, "product revision")?;
            if stored_source_snapshot_id != accepted.source_snapshot_id
                || immutable_revision_payload(&stored_product).ok()
                    != immutable_revision_payload(&accepted).ok()
            {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "An immutable Product revision already exists with different data.",
                ));
            }
            let staging_facts = sqlx::query_scalar::<_, Value>(
                r#"SELECT normalized_payload FROM staging_records
                   WHERE source_snapshot_id=$1 AND source_record_id=$2
                     AND validation_status='valid'
                   ORDER BY created_at DESC LIMIT 1"#,
            )
            .bind(accepted.source_snapshot_id)
            .bind(&accepted.stable_id)
            .fetch_optional(&mut *transaction)
            .await?;
            let mut facts_payload = payload.clone();
            if let Some(staging) = staging_facts.and_then(|value| value.as_object().cloned()) {
                if let Some(target) = facts_payload.as_object_mut() {
                    for key in [
                        "specifications",
                        "operatingConditions",
                        "performanceCurves",
                        "assets",
                    ] {
                        if let Some(value) = staging.get(key) {
                            target.insert(key.into(), value.clone());
                        }
                    }
                }
            }
            project_product_facts(
                &mut transaction,
                accepted.id,
                accepted.current_revision,
                &facts_payload,
                &format!("feishu:{}:{}", accepted.source_revision, accepted.stable_id),
            )
            .await?;
            let presentation_content = serde_json::json!({
                "sortOrder": accepted.sort_order,
                "relatedContentIds": accepted.related_content_ids,
            });
            let presentation_seo = serde_json::to_value(&accepted.seo).map_err(|_| {
                ApiError::internal("Product presentation SEO serialization failed.")
            })?;
            sqlx::query(
                r#"INSERT INTO product_presentation_working
                   (product_id,locale,current_revision,published_revision,slug,title,summary,
                    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                    updated_by,updated_at)
                   VALUES ($1,$2,1,NULL,$3,$4,$5,$6,$7,'draft',false,$8,'editorial',$9,$10)
                   ON CONFLICT (product_id,locale) DO NOTHING"#,
            )
            .bind(accepted.id)
            .bind(&accepted.locale)
            .bind(&accepted.slug)
            .bind(&accepted.title)
            .bind(&accepted.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(accepted.indexable)
            .bind("productMasterIngestion")
            .bind(accepted.updated_at)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"INSERT INTO product_presentation_revisions
                   (product_id,locale,revision,source_product_revision,slug,title,summary,
                    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                    created_by,created_at)
                   VALUES ($1,$2,1,$3,$4,$5,$6,$7,$8,'draft',false,$9,'editorial',$10,$11)
                   ON CONFLICT (product_id,locale,revision) DO NOTHING"#,
            )
            .bind(accepted.id)
            .bind(&accepted.locale)
            .bind(accepted.current_revision)
            .bind(&accepted.slug)
            .bind(&accepted.title)
            .bind(&accepted.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(accepted.indexable)
            .bind("productMasterIngestion")
            .bind(accepted.updated_at)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        }
        if self.pool.is_none() {
            let mut data = self.data.write().await;
            let revisions = data.product_revisions.entry(product.id).or_default();
            if revisions
                .get(&product.current_revision)
                .is_some_and(|stored| {
                    stored.source_snapshot_id != product.source_snapshot_id
                        || immutable_revision_payload(stored).ok()
                            != immutable_revision_payload(product).ok()
                })
            {
                return Err(ApiError::conflict(
                    "An immutable Product revision already exists with different data.",
                ));
            }
            revisions
                .entry(product.current_revision)
                .or_insert_with(|| product.clone());
            data.product_presentations
                .entry((product.id, product.locale.clone()))
                .or_insert_with(|| ProductPresentation {
                    locale: product.locale.clone(),
                    slug: product.slug.clone(),
                    title: product.title.clone(),
                    summary: product.summary.clone(),
                    seo: product.seo.clone(),
                    indexable: product.indexable,
                    sort_order: product.sort_order,
                    related_content_ids: product.related_content_ids.clone(),
                    revision: 1,
                    published_revision: None,
                    updated_at: product.updated_at,
                });
        }
        Ok(())
    }

    pub async fn publish_product_projection(
        &self,
        working: &Product,
        product: &Product,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(product)
            .map_err(|_| ApiError::internal("Product serialization failed."))?;
        let outbox_id = Uuid::new_v4();
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *transaction)
                .await?;
            sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
                .bind(working.id)
                .fetch_optional(&mut *transaction)
                .await?;
            let issues = self
                .postgres_product_publication_issues_on(&mut transaction, working)
                .await?;
            if !issues.is_empty() {
                transaction.rollback().await?;
                return Err(ApiError::validation(issues_as_errors(issues)));
            }
            let presentation = sqlx::query(
                r#"SELECT current_revision,slug,title,summary,content,
                          seo_metadata,is_placeholder,indexable,data_origin,updated_by
                   FROM product_presentation_working
                   WHERE product_id=$1 AND locale=$2
                   FOR UPDATE"#,
            )
            .bind(product.id)
            .bind(&product.locale)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| {
                ApiError::validation(std::collections::BTreeMap::from([(
                    "presentation".into(),
                    vec![
                        "A localized presentation working revision is required before publication."
                            .into(),
                    ],
                )]))
            })?;
            let presentation_revision: i64 = presentation.try_get("current_revision")?;
            let slug: String = presentation.try_get("slug")?;
            let seo: Value = presentation.try_get("seo_metadata")?;
            let canonical_path = seo
                .get("canonicalPath")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(canonical_path) = canonical_path.as_deref() {
                let family = match product.family {
                    crate::models::ProductFamily::Centrifugal => "centrifugal",
                    crate::models::ProductFamily::Axial => "axial",
                    crate::models::ProductFamily::CrossFlow => "cross-flow",
                    crate::models::ProductFamily::InlineDuct => "inline-duct",
                    crate::models::ProductFamily::Motors => "motors",
                };
                let expected_path = format!("/en/products/{family}/{slug}");
                if canonical_path != expected_path
                    || canonical_path.contains(['?', '#', '\\', '\r', '\n', '\0'])
                {
                    transaction.rollback().await?;
                    return Err(ApiError::validation(std::collections::BTreeMap::from([(
                        "seo.canonicalPath".into(),
                        vec![format!("Must exactly match {expected_path}.")],
                    )])));
                }
                let route_owner = sqlx::query_scalar::<_, Uuid>(
                    "SELECT entity_id FROM public_routes WHERE canonical_path=$1",
                )
                .bind(canonical_path)
                .fetch_optional(&mut *transaction)
                .await?;
                if route_owner.is_some_and(|owner| owner != product.id) {
                    transaction.rollback().await?;
                    return Err(ApiError::conflict(
                        "Another published entity already owns this product canonical path.",
                    ));
                }
            }
            let is_placeholder: bool = presentation.try_get("is_placeholder")?;
            let presentation_indexable: bool = presentation.try_get("indexable")?;
            let result = sqlx::query(
                r#"UPDATE products
                   SET status='published', published_revision=current_revision,
                       indexable=$2, payload=$3, updated_at=$4
                   WHERE id=$1 AND current_revision=$5"#,
            )
            .bind(product.id)
            .bind(presentation_indexable && !is_placeholder)
            .bind(&payload)
            .bind(product.updated_at)
            .bind(product.current_revision)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The product changed; reload before publishing.",
                ));
            }
            sqlx::query("DELETE FROM public_routes WHERE entity_type='product' AND entity_id=$1")
                .bind(product.id)
                .execute(&mut *transaction)
                .await?;
            if let Some(canonical_path) = canonical_path.as_deref() {
                sqlx::query(
                    r#"INSERT INTO public_routes
                       (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
                       VALUES ($1,'product',$2,$3,$4,$5,$6)"#,
                )
                .bind(Uuid::new_v4())
                .bind(product.id)
                .bind(&product.locale)
                .bind(canonical_path)
                .bind(presentation_indexable && !is_placeholder)
                .bind(product.updated_at)
                .execute(&mut *transaction)
                .await?;
            }
            sqlx::query(
                r#"INSERT INTO product_localizations
                   (product_id,product_revision,locale,slug,title,summary,content,seo_metadata,
                    translation_state,is_placeholder,indexable,data_origin,updated_by,updated_at)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'verified',$9,$10,$11,$12,$13)
                   ON CONFLICT (product_id,product_revision,locale) DO UPDATE SET
                     slug=EXCLUDED.slug,title=EXCLUDED.title,summary=EXCLUDED.summary,
                     content=EXCLUDED.content,seo_metadata=EXCLUDED.seo_metadata,
                     translation_state='verified',is_placeholder=EXCLUDED.is_placeholder,
                     indexable=EXCLUDED.indexable,data_origin=EXCLUDED.data_origin,
                     updated_by=EXCLUDED.updated_by,updated_at=EXCLUDED.updated_at"#,
            )
            .bind(product.id)
            .bind(product.current_revision)
            .bind(&product.locale)
            .bind(&slug)
            .bind(presentation.try_get::<String, _>("title")?)
            .bind(presentation.try_get::<Option<String>, _>("summary")?)
            .bind(presentation.try_get::<Value, _>("content")?)
            .bind(&seo)
            .bind(is_placeholder)
            .bind(presentation_indexable && !is_placeholder)
            .bind(presentation.try_get::<String, _>("data_origin")?)
            .bind(presentation.try_get::<String, _>("updated_by")?)
            .bind(product.updated_at)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"UPDATE product_presentation_working
                   SET published_revision=current_revision,translation_state='verified'
                   WHERE product_id=$1 AND locale=$2 AND current_revision=$3"#,
            )
            .bind(product.id)
            .bind(&product.locale)
            .bind(presentation_revision)
            .execute(&mut *transaction)
            .await?;
            let outbox_payload = serde_json::json!({
                "entityId": product.id,
                "factRevision": product.current_revision,
                "presentationRevision": presentation_revision,
                "locale": product.locale
            });
            sqlx::query(
                r#"INSERT INTO outbox_events
                   (id, topic, aggregate_type, aggregate_id, payload)
                   VALUES ($1,'public.product.published','product',$2,$3)
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(outbox_id)
            .bind(product.id)
            .bind(&outbox_payload)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        } else {
            let mut issues = validate_product_master(working);
            issues.extend(self.in_memory_product_publication_issues(working).await);
            if !issues.is_empty() {
                return Err(ApiError::validation(issues_as_errors(issues)));
            }
        }
        if self.pool.is_none() {
            let mut data = self.data.write().await;
            let mut public_product = product.clone();
            let presentation_revision = if let Some(presentation) = data
                .product_presentations
                .get_mut(&(product.id, product.locale.clone()))
            {
                presentation.published_revision = Some(presentation.revision);
                public_product.slug = presentation.slug.clone();
                public_product.title = presentation.title.clone();
                public_product.summary = presentation.summary.clone();
                public_product.seo = presentation.seo.clone();
                public_product.indexable = presentation.indexable;
                public_product.sort_order = presentation.sort_order;
                public_product.related_content_ids = presentation.related_content_ids.clone();
                presentation.revision
            } else {
                1
            };
            data.published_products.insert(product.id, public_product);
            let outbox_payload = serde_json::json!({
                "entityId": product.id,
                "factRevision": product.current_revision,
                "presentationRevision": presentation_revision,
                "locale": product.locale
            });
            if !data.outbox_events.iter().any(|value| {
                value.get("topic").and_then(Value::as_str) == Some("public.product.published")
                    && value.get("aggregateId") == Some(&serde_json::json!(product.id))
                    && value.pointer("/payload/factRevision")
                        == Some(&serde_json::json!(product.current_revision))
                    && value.pointer("/payload/presentationRevision")
                        == Some(&serde_json::json!(presentation_revision))
            }) {
                data.outbox_events.push(serde_json::json!({
                    "id": outbox_id,
                    "topic": "public.product.published",
                    "aggregateId": product.id,
                    "payload": outbox_payload
                }));
            }
        }
        Ok(())
    }

    pub async fn persist_override(&self, value: &TemporaryOverride) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO product_temporary_overrides
               (id, product_id, field_path, value, reason, created_at, expires_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
        )
        .bind(value.id)
        .bind(value.product_id)
        .bind(&value.field_path)
        .bind(&value.value)
        .bind(&value.reason)
        .bind(value.created_at)
        .bind(value.expires_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn persist_rfq(&self, value: &RfqSubmission) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO rfq_submissions
               (id, reference, journey, status, source_path, locale, submitted_at,
                retention_until, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(value.id)
        .bind(&value.reference)
        .bind(enum_label(value.request.journey))
        .bind(&value.status)
        .bind(&value.request.source_path)
        .bind(&value.request.locale)
        .bind(value.submitted_at)
        .bind(value.retention_until)
        .bind(
            serde_json::to_value(value)
                .map_err(|_| ApiError::internal("RFQ serialization failed."))?,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn persist_contact(&self, value: &ContactRequest) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO contact_requests
               (id, reference, topic, status, source_path, locale, submitted_at,
                retention_until, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(value.id)
        .bind(&value.reference)
        .bind(&value.request.topic)
        .bind(&value.status)
        .bind(&value.request.source_path)
        .bind(&value.request.locale)
        .bind(value.submitted_at)
        .bind(value.retention_until)
        .bind(
            serde_json::to_value(value)
                .map_err(|_| ApiError::internal("Contact serialization failed."))?,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn persist_analytics_consent(
        &self,
        receipt: &AnalyticsConsentReceipt,
    ) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let storage_session_id = self.analytics_storage_session_id(receipt.anonymous_session_id)?;
        sqlx::query(
            r#"INSERT INTO consent_records
               (id, anonymous_session_id, policy_version, analytics_allowed, granted_at, expires_at)
               VALUES ($1,$2,$3,$4,$5,$6)"#,
        )
        .bind(receipt.consent_receipt)
        .bind(storage_session_id)
        .bind(&receipt.policy_version)
        .bind(receipt.analytics_allowed)
        .bind(receipt.granted_at)
        .bind(receipt.expires_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Returns a consent receipt only when it is the latest decision for its
    /// anonymous session. A later denial therefore invalidates every older
    /// allow receipt without needing a separate revocation flag.
    pub async fn current_analytics_consent(
        &self,
        receipt_id: Uuid,
    ) -> Result<Option<AnalyticsConsentReceipt>, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                r#"SELECT requested.id, requested.anonymous_session_id,
                          requested.policy_version, requested.analytics_allowed,
                          requested.granted_at, requested.expires_at
                   FROM consent_records requested
                   WHERE requested.id = $1
                     AND requested.id = (
                       SELECT latest.id
                       FROM consent_records latest
                       WHERE latest.anonymous_session_id = requested.anonymous_session_id
                       ORDER BY latest.granted_at DESC, latest.id DESC
                       LIMIT 1
                     )"#,
            )
            .bind(receipt_id)
            .fetch_optional(pool)
            .await?;
            return row
                .map(|row| {
                    Ok(AnalyticsConsentReceipt {
                        consent_receipt: row.try_get("id")?,
                        anonymous_session_id: row.try_get("anonymous_session_id")?,
                        policy_version: row.try_get("policy_version")?,
                        analytics_allowed: row.try_get("analytics_allowed")?,
                        granted_at: row.try_get("granted_at")?,
                        expires_at: row.try_get("expires_at")?,
                    })
                })
                .transpose();
        }

        let data = self.data.read().await;
        let requested = match data.analytics_consents.get(&receipt_id) {
            Some(receipt) => receipt,
            None => return Ok(None),
        };
        let latest = data
            .analytics_consents
            .values()
            .filter(|candidate| candidate.anonymous_session_id == requested.anonymous_session_id)
            .max_by(|left, right| {
                left.granted_at
                    .cmp(&right.granted_at)
                    .then_with(|| left.consent_receipt.cmp(&right.consent_receipt))
            });
        Ok(latest
            .filter(|latest| latest.consent_receipt == receipt_id)
            .cloned())
    }

    pub async fn persist_analytics_event(
        &self,
        event_id: Uuid,
        event: &Value,
    ) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let external_session_id = event
            .get("anonymousSessionId")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| ApiError::bad_request("anonymousSessionId is invalid."))?;
        let consent_record_id = event
            .get("consentReceipt")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| ApiError::bad_request("consentReceipt is invalid."))?;
        let storage_session_id = self.analytics_storage_session_id(external_session_id)?;
        let mut transaction = pool.begin().await?;
        let guest_visit_id = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM guest_visits
               WHERE anonymous_session_id=$1 AND consent_record_id=$2
                 AND consent_analytics_allowed=true AND retention_until > now()
               ORDER BY last_seen_at DESC LIMIT 1"#,
        )
        .bind(storage_session_id)
        .bind(consent_record_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| {
            ApiError::validation(BTreeMap::from([(
                "anonymousSessionId".into(),
                vec!["Create the consented guest visit before sending analytics events.".into()],
            )]))
        })?;
        sqlx::query(
            r#"INSERT INTO analytics_events
               (id, event_name, source_path, locale, anonymous_session_id, properties,
                occurred_at,guest_visit_id,consent_record_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(event_id)
        .bind(
            event
                .get("eventName")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .bind(
            event
                .get("sourcePath")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .bind(event.get("locale").and_then(Value::as_str).unwrap_or("en"))
        .bind(storage_session_id)
        .bind(
            event
                .get("properties")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        )
        .bind(Utc::now())
        .bind(guest_visit_id)
        .bind(consent_record_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn persist_operation(&self, operation: &BackgroundOperation) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let mut transaction = pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO operation_runs
               (id, kind, status, reason, result, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7)
               ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status,
                 result=EXCLUDED.result,updated_at=EXCLUDED.updated_at"#,
        )
        .bind(operation.id)
        .bind(enum_label(operation.kind))
        .bind(enum_label(operation.status))
        .bind(&operation.reason)
        .bind(&operation.result)
        .bind(operation.created_at)
        .bind(operation.updated_at)
        .execute(&mut *transaction)
        .await?;
        // Product Master uploads are validated and promoted before the 202 is
        // returned in the current phase-one implementation. Never enqueue a
        // second orphan job containing an operation-only payload; a durable
        // worker import will use a private encrypted staging reference instead.
        if operation.status == crate::models::OperationStatus::Queued
            && operation.kind != crate::models::OperationKind::ProductImport
        {
            sqlx::query(
                r#"INSERT INTO jobs (id, job_type, status, payload, available_at, created_at, updated_at)
                   VALUES ($1,$2,'queued',$3,$4,$4,$4) ON CONFLICT (id) DO NOTHING"#,
            )
            .bind(operation.id)
            .bind(enum_label(operation.kind))
            .bind(serde_json::to_value(operation).map_err(|_| {
                ApiError::internal("Operation serialization failed.")
            })?)
            .bind(operation.created_at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn list_operations(&self) -> Result<Vec<BackgroundOperation>, ApiError> {
        if let Some(pool) = &self.pool {
            let rows = sqlx::query(
                "SELECT id, kind, status, reason, result, created_at, updated_at FROM operation_runs ORDER BY created_at DESC",
            )
            .fetch_all(pool)
            .await?;
            return rows
                .into_iter()
                .map(|row| {
                    Ok(BackgroundOperation {
                        id: row.try_get("id")?,
                        kind: decode_enum(row.try_get("kind")?, "operation kind")?,
                        status: decode_enum(row.try_get("status")?, "operation status")?,
                        reason: row.try_get("reason")?,
                        result: row.try_get("result")?,
                        created_at: row.try_get("created_at")?,
                        updated_at: row.try_get("updated_at")?,
                    })
                })
                .collect();
        }
        let mut values: Vec<_> = self
            .data
            .read()
            .await
            .operations
            .values()
            .cloned()
            .collect();
        values.sort_by_key(|operation| std::cmp::Reverse(operation.created_at));
        Ok(values)
    }

    pub async fn get_operation(
        &self,
        operation_id: Uuid,
    ) -> Result<Option<BackgroundOperation>, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                "SELECT id, kind, status, reason, result, created_at, updated_at FROM operation_runs WHERE id=$1",
            )
            .bind(operation_id)
            .fetch_optional(pool)
            .await?;
            return row
                .map(|row| {
                    Ok(BackgroundOperation {
                        id: row.try_get("id")?,
                        kind: decode_enum(row.try_get("kind")?, "operation kind")?,
                        status: decode_enum(row.try_get("status")?, "operation status")?,
                        reason: row.try_get("reason")?,
                        result: row.try_get("result")?,
                        created_at: row.try_get("created_at")?,
                        updated_at: row.try_get("updated_at")?,
                    })
                })
                .transpose();
        }
        Ok(self
            .data
            .read()
            .await
            .operations
            .get(&operation_id)
            .cloned())
    }

    pub async fn persist_audit(&self, event: AuditEvent) -> Result<(), ApiError> {
        if let Some(pool) = &self.pool {
            sqlx::query(
                r#"INSERT INTO audit_log
                   (id, actor, action, entity_type, entity_id, before_value, after_value,
                    reason, request_id, occurred_at)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
            )
            .bind(event.id)
            .bind(&event.actor)
            .bind(&event.action)
            .bind(&event.entity_type)
            .bind(event.entity_id)
            .bind(&event.before)
            .bind(&event.after)
            .bind(&event.reason)
            .bind(event.request_id)
            .bind(event.occurred_at)
            .execute(pool)
            .await?;
        } else {
            self.data.write().await.audit_events.push(event);
        }
        Ok(())
    }
}

fn decode_platform_settings_row(row: &PgRow) -> Result<PlatformSettings, ApiError> {
    fn json_value(row: &PgRow, column: &str) -> Result<Value, ApiError> {
        row.try_get::<Option<Value>, _>(column)?.ok_or_else(|| {
            ApiError::service_unavailable(format!("The `{column}` platform setting is missing."))
        })
    }

    fn integer(value: Value, key: &str, minimum: i64, maximum: i64) -> Result<i64, ApiError> {
        value
            .as_i64()
            .filter(|value| (minimum..=maximum).contains(value))
            .ok_or_else(|| {
                ApiError::service_unavailable(format!(
                    "The `{key}` setting must be an integer between {minimum} and {maximum}."
                ))
            })
    }

    let public_locale = json_value(row, "public_locale")?
        .as_str()
        .filter(|value| *value == "en")
        .map(str::to_owned)
        .ok_or_else(|| {
            ApiError::service_unavailable("The `publicLocale` setting must remain `en`.")
        })?;
    Ok(PlatformSettings {
        rfq_retention_days: integer(
            json_value(row, "rfq_retention_days")?,
            "rfqRetentionDays",
            30,
            3_650,
        )?,
        retention_deletion_grace_days: integer(
            json_value(row, "retention_deletion_grace_days")?,
            "retentionDeletionGraceDays",
            1,
            365,
        )?,
        temporary_override_default_days: integer(
            json_value(row, "temporary_override_default_days")?,
            "temporaryOverrideDefaultDays",
            1,
            365,
        )?,
        public_locale,
        revision: row.try_get("revision")?,
    })
}

fn apply_platform_settings_update(
    before: &PlatformSettings,
    update: &UpdatePlatformSettings,
) -> Result<PlatformSettings, ApiError> {
    let mut errors = BTreeMap::new();
    let reason = update.reason.trim();
    if reason.chars().count() < 12 {
        errors.insert(
            "reason".into(),
            vec!["Reason must contain at least 12 characters.".into()],
        );
    }
    for (field, value, minimum, maximum) in [
        ("rfqRetentionDays", update.rfq_retention_days, 30, 3_650),
        (
            "retentionDeletionGraceDays",
            update.retention_deletion_grace_days,
            1,
            365,
        ),
        (
            "temporaryOverrideDefaultDays",
            update.temporary_override_default_days,
            1,
            365,
        ),
    ] {
        if value.is_some_and(|value| !(minimum..=maximum).contains(&value)) {
            errors.insert(
                field.into(),
                vec![format!("Value must be between {minimum} and {maximum}.")],
            );
        }
    }
    if update.rfq_retention_days.is_none()
        && update.retention_deletion_grace_days.is_none()
        && update.temporary_override_default_days.is_none()
    {
        errors.insert(
            "settings".into(),
            vec!["At least one mutable setting is required.".into()],
        );
    }
    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    let mut after = before.clone();
    if let Some(value) = update.rfq_retention_days {
        after.rfq_retention_days = value;
    }
    if let Some(value) = update.retention_deletion_grace_days {
        after.retention_deletion_grace_days = value;
    }
    if let Some(value) = update.temporary_override_default_days {
        after.temporary_override_default_days = value;
    }
    if after.rfq_retention_days == before.rfq_retention_days
        && after.retention_deletion_grace_days == before.retention_deletion_grace_days
        && after.temporary_override_default_days == before.temporary_override_default_days
    {
        let mut errors = BTreeMap::new();
        errors.insert(
            "settings".into(),
            vec!["At least one setting must change.".into()],
        );
        return Err(ApiError::validation(errors));
    }
    after.revision = before
        .revision
        .checked_add(1)
        .ok_or_else(|| ApiError::service_unavailable("Settings revision is exhausted."))?;
    Ok(after)
}

fn settings_audit_event(
    before: &PlatformSettings,
    after: &PlatformSettings,
    update: &UpdatePlatformSettings,
    actor: &str,
    request_id: Uuid,
    occurred_at: DateTime<Utc>,
) -> AuditEvent {
    AuditEvent {
        id: Uuid::new_v4(),
        actor: actor.into(),
        action: "settings.update".into(),
        entity_type: "platformSettings".into(),
        entity_id: None,
        before: serde_json::to_value(before).ok(),
        after: serde_json::to_value(after).ok(),
        reason: Some(update.reason.trim().into()),
        request_id,
        occurred_at,
    }
}

async fn insert_audit_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &AuditEvent,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id, actor, action, entity_type, entity_id, before_value, after_value,
            reason, request_id, occurred_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
    )
    .bind(event.id)
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.entity_type)
    .bind(event.entity_id)
    .bind(&event.before)
    .bind(&event.after)
    .bind(&event.reason)
    .bind(event.request_id)
    .bind(event.occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn overlay_presentation(product: &mut Product, presentation: &ProductPresentation) {
    product.locale = presentation.locale.clone();
    product.slug = presentation.slug.clone();
    product.title = presentation.title.clone();
    product.summary = presentation.summary.clone();
    product.seo = presentation.seo.clone();
    product.indexable = presentation.indexable;
    product.sort_order = presentation.sort_order;
    product.related_content_ids = presentation.related_content_ids.clone();
}

fn overlay_presentation_row(product: &mut Product, row: &PgRow) -> Result<(), ApiError> {
    if row
        .try_get::<Option<i64>, _>("presentation_revision")?
        .is_none()
    {
        return Ok(());
    }
    product.locale = row.try_get("presentation_locale")?;
    product.slug = row.try_get("presentation_slug")?;
    product.title = row.try_get("presentation_title")?;
    product.summary = row.try_get("presentation_summary")?;
    product.seo = decode_payload(row.try_get("presentation_seo")?, "product presentation SEO")?;
    product.indexable = row.try_get("presentation_indexable")?;
    let content: Value = row.try_get("presentation_content")?;
    product.sort_order = content
        .get("sortOrder")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or_default();
    product.related_content_ids = content
        .get("relatedContentIds")
        .cloned()
        .map(|value| decode_payload(value, "product related content ids"))
        .transpose()?
        .unwrap_or_default();
    Ok(())
}

fn enum_label<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn decode_payload<T: serde::de::DeserializeOwned>(
    value: Value,
    entity: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| {
        tracing::error!(%error, entity, "stored JSON payload is invalid");
        ApiError::service_unavailable(format!("Stored {entity} data is invalid."))
    })
}

fn decode_enum<T: serde::de::DeserializeOwned>(value: String, entity: &str) -> Result<T, ApiError> {
    decode_payload(Value::String(value), entity)
}

fn text_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{derive_analytics_storage_id, AppState};
    use crate::config::Config;
    use uuid::Uuid;

    #[test]
    fn analytics_session_storage_identifier_is_keyed_and_deterministic() {
        let external = Uuid::parse_str("0fe2d520-0f8d-49bb-b858-f984d46c9fea").unwrap();
        let first = derive_analytics_storage_id(external, &[0x11; 32]);
        let repeated = derive_analytics_storage_id(external, &[0x11; 32]);
        let rotated = derive_analytics_storage_id(external, &[0x22; 32]);
        assert_eq!(first, repeated);
        assert_ne!(first, external);
        assert_ne!(first, rotated);
        assert_eq!(first.get_version_num(), 8);
    }

    #[test]
    fn production_state_refuses_the_in_memory_adapter() {
        let mut config = Config::for_test();
        config.production = true;
        config.database_url = None;
        assert!(AppState::new(config).is_err());
    }
}
