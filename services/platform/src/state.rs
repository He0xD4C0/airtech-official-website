use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Weak},
};

use chrono::{DateTime, Utc};
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
        ContactRequest, ContentEntry, PlatformSettings, Product, RfqSubmission, SourceSnapshot,
        StagingRecord, StagingValidationStatus, SyncConflict, SyncRun, SyncRunStatus,
        TemporaryOverride, UpdatePlatformSettings, ValidationIssue,
    },
    rate_limit::InMemoryRateLimit,
    services::product_publication::{
        immutable_revision_payload, issues_as_errors, validate_accepted_staging_payload,
        validate_product_master, validate_source_owned_alignment, workflow_error,
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

#[derive(Default)]
pub struct PlatformData {
    pub settings: PlatformSettings,
    pub content: HashMap<Uuid, ContentEntry>,
    pub content_revisions: HashMap<Uuid, BTreeMap<i64, ContentEntry>>,
    pub published_content: HashMap<Uuid, ContentEntry>,
    pub products: HashMap<Uuid, Product>,
    pub product_revisions: HashMap<Uuid, BTreeMap<i64, Product>>,
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
    pub fn new(config: Config) -> Result<Self, ApiError> {
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

            let mut data = self.data.write().await;
            data.settings = after.clone();
            data.audit_events.push(event);
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
        let mut issues = validate_product_master(product);
        if self.pool.is_some() {
            issues.extend(self.postgres_product_publication_issues(product).await?);
        } else {
            issues.extend(self.in_memory_product_publication_issues(product).await);
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ApiError::validation(issues_as_errors(issues)))
        }
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
                      product.current_revision AS working_revision,
                      revision.payload AS immutable_payload,
                      revision.source_snapshot_id AS immutable_source_snapshot_id
               FROM products AS product
               LEFT JOIN product_revisions AS revision
                 ON revision.product_id=product.id
                AND revision.revision=product.current_revision
               WHERE product.id=$1"#,
        )
        .bind(product.id)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(row) = working_row {
            let working_payload: Value = row.try_get("working_payload")?;
            let stored_working: Product = decode_payload(working_payload, "working product")?;
            let immutable_payload: Option<Value> = row.try_get("immutable_payload")?;
            let immutable_source_snapshot_id: Option<Uuid> =
                row.try_get("immutable_source_snapshot_id")?;
            let immutable_matches = immutable_payload
                .and_then(|payload| decode_payload::<Product>(payload, "product revision").ok())
                .is_some_and(|revision| {
                    immutable_source_snapshot_id == Some(product.source_snapshot_id)
                        && immutable_revision_payload(&revision).ok()
                            == immutable_revision_payload(product).ok()
                });
            if row.try_get::<i64, _>("working_revision")? != product.current_revision
                || row.try_get::<Uuid, _>("working_source_snapshot_id")?
                    != product.source_snapshot_id
                || row.try_get::<String, _>("working_source_revision")?.trim()
                    != product.source_revision.trim()
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
            issues.extend(validate_product_master(&stored_working));
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

    pub async fn hydrate(&self) -> Result<(), ApiError> {
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
            sqlx::query(
                r#"INSERT INTO content_revisions
               (content_id, revision, payload, created_by, created_at)
               VALUES ($1,$2,$3,$4,$5) ON CONFLICT (content_id, revision) DO NOTHING"#,
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
        self.data
            .write()
            .await
            .content_revisions
            .entry(entry.id)
            .or_default()
            .entry(entry.current_revision)
            .or_insert_with(|| entry.clone());
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

    pub async fn persist_content_update(
        &self,
        entry: &ContentEntry,
        actor: &str,
        expected_revision: i64,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content serialization failed."))?;
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            let result = sqlx::query(
                r#"UPDATE content_entries SET kind=$2, slug=$3, locale=$4, title=$5,
                   status=$6, is_placeholder=$7, current_revision=$8,
                   published_revision=$9, scheduled_for=$10, payload=$11, updated_at=$12
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
            sqlx::query(
                r#"INSERT INTO content_revisions
                   (content_id, revision, payload, created_by, created_at)
                   VALUES ($1,$2,$3,$4,$5)"#,
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
        self.data
            .write()
            .await
            .content_revisions
            .entry(entry.id)
            .or_default()
            .entry(entry.current_revision)
            .or_insert_with(|| entry.clone());
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
                       scheduled_for=NULL, payload=$8, updated_at=$9
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
        let payload = serde_json::to_value(product)
            .map_err(|_| ApiError::internal("Product serialization failed."))?;
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                r#"INSERT INTO products
               (id, stable_id, model, slug, locale, family, source_snapshot_id,
                source_revision, status, current_revision, published_revision,
                indexable, payload, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
               ON CONFLICT (id) DO UPDATE SET stable_id=EXCLUDED.stable_id,
               model=EXCLUDED.model, slug=EXCLUDED.slug, locale=EXCLUDED.locale,
               family=EXCLUDED.family, source_snapshot_id=EXCLUDED.source_snapshot_id,
               source_revision=EXCLUDED.source_revision, status=EXCLUDED.status,
               current_revision=EXCLUDED.current_revision,
               published_revision=EXCLUDED.published_revision,
               indexable=EXCLUDED.indexable, payload=EXCLUDED.payload,
               updated_at=EXCLUDED.updated_at"#,
            )
            .bind(product.id)
            .bind(&product.stable_id)
            .bind(&product.model)
            .bind(&product.slug)
            .bind(&product.locale)
            .bind(enum_label(product.family))
            .bind(product.source_snapshot_id)
            .bind(&product.source_revision)
            .bind(enum_label(product.status))
            .bind(product.current_revision)
            .bind(product.published_revision)
            .bind(product.indexable)
            .bind(&payload)
            .bind(product.updated_at)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"INSERT INTO product_revisions
               (product_id, revision, source_snapshot_id, payload, created_at)
               VALUES ($1,$2,$3,$4,$5) ON CONFLICT (product_id, revision) DO NOTHING"#,
            )
            .bind(product.id)
            .bind(product.current_revision)
            .bind(product.source_snapshot_id)
            .bind(&payload)
            .bind(product.updated_at)
            .execute(&mut *transaction)
            .await?;
            let stored_revision = sqlx::query(
                r#"SELECT source_snapshot_id, payload
                   FROM product_revisions
                   WHERE product_id=$1 AND revision=$2"#,
            )
            .bind(product.id)
            .bind(product.current_revision)
            .fetch_one(&mut *transaction)
            .await?;
            let stored_source_snapshot_id: Uuid = stored_revision.try_get("source_snapshot_id")?;
            let stored_payload: Value = stored_revision.try_get("payload")?;
            let stored_product: Product = decode_payload(stored_payload, "product revision")?;
            if stored_source_snapshot_id != product.source_snapshot_id
                || immutable_revision_payload(&stored_product).ok()
                    != immutable_revision_payload(product).ok()
            {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "An immutable Product revision already exists with different data.",
                ));
            }
            transaction.commit().await?;
        }
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
        let outbox_payload = serde_json::json!({
            "entityId": product.id,
            "revision": product.current_revision,
            "locale": product.locale
        });
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *transaction)
                .await?;
            sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
                .bind(working.id)
                .fetch_optional(&mut *transaction)
                .await?;
            let mut issues = validate_product_master(working);
            issues.extend(
                self.postgres_product_publication_issues_on(&mut transaction, working)
                    .await?,
            );
            if !issues.is_empty() {
                transaction.rollback().await?;
                return Err(ApiError::validation(issues_as_errors(issues)));
            }
            let result = sqlx::query(
                r#"UPDATE products
                   SET status='published', published_revision=current_revision,
                       indexable=$2, payload=$3, updated_at=$4
                   WHERE id=$1 AND current_revision=$5"#,
            )
            .bind(product.id)
            .bind(product.indexable)
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
        let mut data = self.data.write().await;
        data.published_products.insert(product.id, product.clone());
        if !data.outbox_events.iter().any(|value| {
            value.get("topic").and_then(Value::as_str) == Some("public.product.published")
                && value.get("aggregateId") == Some(&serde_json::json!(product.id))
                && value.pointer("/payload/revision")
                    == Some(&serde_json::json!(product.current_revision))
        }) {
            data.outbox_events.push(serde_json::json!({
                "id": outbox_id,
                "topic": "public.product.published",
                "aggregateId": product.id,
                "payload": outbox_payload
            }));
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
        sqlx::query(
            r#"INSERT INTO consent_records
               (id, anonymous_session_id, policy_version, analytics_allowed, granted_at, expires_at)
               VALUES ($1,$2,$3,$4,$5,$6)"#,
        )
        .bind(receipt.consent_receipt)
        .bind(receipt.anonymous_session_id)
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
        sqlx::query(
            r#"INSERT INTO analytics_events
               (id, event_name, source_path, locale, anonymous_session_id, properties,
                occurred_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
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
        .bind(
            event
                .get("anonymousSessionId")
                .and_then(Value::as_str)
                .and_then(|v| Uuid::parse_str(v).ok()),
        )
        .bind(
            event
                .get("properties")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        )
        .bind(Utc::now())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn persist_operation(&self, operation: &BackgroundOperation) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO operation_runs
               (id, kind, status, reason, result, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
        )
        .bind(operation.id)
        .bind(enum_label(operation.kind))
        .bind(enum_label(operation.status))
        .bind(&operation.reason)
        .bind(&operation.result)
        .bind(operation.created_at)
        .bind(operation.updated_at)
        .execute(pool)
        .await?;
        sqlx::query(
            r#"INSERT INTO jobs (id, job_type, status, payload, available_at, created_at, updated_at)
               VALUES ($1,$2,'queued',$3,$4,$4,$4)"#,
        )
        .bind(operation.id)
        .bind(enum_label(operation.kind))
        .bind(serde_json::to_value(operation).map_err(|_| ApiError::internal("Operation serialization failed."))?)
        .bind(operation.created_at)
        .execute(pool)
        .await?;
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
        }
        self.data.write().await.audit_events.push(event);
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
