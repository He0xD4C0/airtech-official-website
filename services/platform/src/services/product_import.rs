use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Duration, Utc};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    config::{ApprovedProductMaster, ProductStagingEncryptionKey},
    error::ApiError,
    models::{MissingAssetReference, ProductImportError, ProductImportResult},
    services::product_facts::project_product_facts,
};

const MAX_CSV_BYTES: usize = 16 * 1024 * 1024;
const MAX_ROWS: usize = 50_000;

#[derive(Clone, Debug)]
pub struct ImportedProductRow {
    pub row_number: i32,
    pub stable_id: String,
    pub normalized_payload: Value,
    /// Nonce-prefixed AES-256-GCM ciphertext. This value is deliberately not
    /// serializable and is persisted only in the private staging table.
    pub confidential_payload: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct ParsedProductImport {
    pub result: ProductImportResult,
    pub rows: Vec<ImportedProductRow>,
}

#[derive(Clone, Copy, Debug)]
pub struct PrivatePricingEnvelope<'a> {
    pub mapping_version: &'a str,
    pub checksum: &'a str,
    pub source_row_number: i32,
    pub stable_id: &'a str,
    pub nonce: &'a [u8],
    pub ciphertext: &'a [u8],
    pub authentication_tag: &'a [u8],
}

/// Result of durably accepting a Product Master import. The operation id is
/// intentionally the same UUID as the import run and background job so a
/// retry cannot create detached jobs or ambiguous progress streams.
#[derive(Clone, Debug)]
pub struct StagedProductImport {
    pub operation_id: Uuid,
    pub result: ProductImportResult,
    pub queued: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductMasterAuthorityDecision {
    ProductionApproved,
    ConfiguredApprovedDevelopment,
    NonProductionValidation,
}

impl ProductMasterAuthorityDecision {
    fn audit_label(self) -> &'static str {
        match self {
            Self::ProductionApproved => "productionApprovedSource",
            Self::ConfiguredApprovedDevelopment => "configuredApprovedDevelopmentSource",
            Self::NonProductionValidation => "nonProductionValidation",
        }
    }
}

/// Apply the exact source decision before staging, again in the worker, and
/// again before publication. Production never infers authority from a
/// successfully parsed shape or from an earlier database status alone.
pub fn verify_product_master_authority(
    environment: &str,
    approved: Option<&ApprovedProductMaster>,
    checksum: &str,
    mapping_version: &str,
    valid_rows: i64,
    error_rows: i64,
) -> Result<ProductMasterAuthorityDecision, ApiError> {
    if !matches!(environment, "production" | "development" | "test") {
        return Err(ApiError::bad_request(
            "Product import environment must be production, development, or test.",
        ));
    }
    let matches_approved = approved.is_some_and(|authority| {
        authority.sha256.eq_ignore_ascii_case(checksum)
            && authority.mapping_version == mapping_version
            && authority.expected_valid_rows == valid_rows
            && authority.expected_error_rows == error_rows
    });
    if environment == "production" {
        if approved.is_none() {
            return Err(ApiError::service_unavailable(
                "Production Product Master authority is not configured.",
            ));
        }
        if !matches_approved {
            return Err(ApiError::conflict(
                "Product Master source, mapping, or reviewed row counts do not match the production authority registration.",
            ));
        }
        return Ok(ProductMasterAuthorityDecision::ProductionApproved);
    }
    if approved.is_some() && !matches_approved {
        return Err(ApiError::conflict(
            "Product Master source, mapping, or reviewed row counts do not match the configured authority registration.",
        ));
    }
    Ok(if matches_approved {
        ProductMasterAuthorityDecision::ConfiguredApprovedDevelopment
    } else {
        ProductMasterAuthorityDecision::NonProductionValidation
    })
}

/// Persist the encrypted source rows and the allow-listed normalized rows,
/// then enqueue a worker job which refers only to the import run UUID.
///
/// The caller must parse with [`parse_product_master`] before entering this
/// function. Plaintext CSV and plaintext source rows never enter
/// `operation_runs`, `jobs`, `audit_log`, or tracing fields.
pub async fn stage_and_queue_product_import(
    pool: &PgPool,
    mut parsed: ParsedProductImport,
    environment: &str,
    approved: Option<&ApprovedProductMaster>,
    actor_id: Option<Uuid>,
    audit_actor: &str,
) -> Result<StagedProductImport, ApiError> {
    let authority_decision = verify_product_master_authority(
        environment,
        approved,
        &parsed.result.checksum,
        &parsed.result.mapping_version,
        parsed.result.valid_rows,
        parsed.result.malformed_rows,
    )?;
    parsed.result.status = if parsed.result.valid_rows > 0 {
        "readyToPublish".into()
    } else {
        "failed".into()
    };
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "verifiedCsv:{environment}:{}:{}",
            parsed.result.checksum, parsed.result.mapping_version,
        ))
        .execute(&mut *transaction)
        .await?;

    let mut existing = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM product_import_runs
           WHERE environment=$1 AND data_origin='verifiedCsv'
             AND source_checksum=$2 AND mapping_version=$3"#,
    )
    .bind(environment)
    .bind(&parsed.result.checksum)
    .bind(&parsed.result.mapping_version)
    .fetch_optional(&mut *transaction)
    .await?;
    let mut claimed_legacy = false;
    if existing.is_none() {
        let legacy = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM product_import_runs
               WHERE environment='legacy' AND data_origin='verifiedCsv'
                 AND source_checksum=$1 AND mapping_version=$2
               ORDER BY created_at,id
               FOR UPDATE LIMIT 1"#,
        )
        .bind(&parsed.result.checksum)
        .bind(&parsed.result.mapping_version)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(legacy_id) = legacy {
            existing = sqlx::query_scalar::<_, Uuid>(
                r#"UPDATE product_import_runs SET environment=$2
                   WHERE id=$1 AND environment='legacy'
                   RETURNING id"#,
            )
            .bind(legacy_id)
            .bind(environment)
            .fetch_optional(&mut *transaction)
            .await?;
            claimed_legacy = existing.is_some();
            if claimed_legacy {
                sqlx::query(
                    r#"INSERT INTO audit_log
                       (id,actor,action,entity_type,entity_id,before_value,
                        after_value,reason,request_id,occurred_at)
                       VALUES ($1,$2,'productMaster.import.environmentClaim',
                               'productImport',$3,$4,$5,$6,$7,$8)"#,
                )
                .bind(Uuid::new_v4())
                .bind(audit_actor)
                .bind(legacy_id)
                .bind(json!({"environment": "legacy"}))
                .bind(json!({"environment": environment, "actorId": actor_id}))
                .bind("Attribute a pre-0008 Product import to its first verified runtime environment.")
                .bind(Uuid::new_v4())
                .bind(Utc::now())
                .execute(&mut *transaction)
                .await?;
            }
        }
    }

    if let Some(existing) = existing {
        if claimed_legacy {
            transaction.commit().await?;
        } else {
            transaction.rollback().await?;
        }
        let mut result = load_product_import_result(pool, existing)
            .await?
            .ok_or_else(|| ApiError::service_unavailable("Stored product import is incomplete."))?;
        verify_product_master_authority(
            environment,
            approved,
            &result.checksum,
            &result.mapping_version,
            result.valid_rows,
            result.malformed_rows,
        )?;
        result.reused = true;
        let queued = matches!(
            result.status.as_str(),
            "queued" | "receiving" | "validating" | "readyToPublish"
        );
        return Ok(StagedProductImport {
            operation_id: existing,
            result,
            queued,
        });
    }

    let created_at = parsed.result.created_at;
    let run_id = parsed.result.id;
    let queued = parsed.result.valid_rows > 0;
    let error_count = parsed
        .result
        .errors
        .iter()
        .filter(|error| error.severity == "error")
        .count() as i64;
    sqlx::query(
        r#"INSERT INTO product_import_runs
           (id,environment,data_origin,dry_run,status,mapping_version,source_checksum,
            records_received,records_valid,error_count,created_by,started_at,completed_at,created_at)
           VALUES ($1,$2,'verifiedCsv',false,$3,$4,$5,$6,$7,$8,$9,$10,
                   CASE WHEN $11 THEN NULL ELSE $10 END,$10)"#,
    )
    .bind(run_id)
    .bind(environment)
    .bind(&parsed.result.status)
    .bind(&parsed.result.mapping_version)
    .bind(&parsed.result.checksum)
    .bind(parsed.result.total_rows)
    .bind(parsed.result.valid_rows)
    .bind(error_count)
    .bind(actor_id)
    .bind(created_at)
    .bind(queued)
    .execute(&mut *transaction)
    .await?;

    for row in &parsed.rows {
        persist_staged_row(&mut transaction, &parsed, row).await?;
    }
    persist_import_findings(&mut transaction, &parsed).await?;
    persist_source_resolution_audit(&mut transaction, &parsed, audit_actor, actor_id).await?;

    let operation_status = if queued { "queued" } else { "failed" };
    let operation_result = (!queued).then(|| json!({"import": parsed.result}));
    sqlx::query(
        r#"INSERT INTO operation_runs
           (id,kind,status,reason,result,created_at,updated_at)
           VALUES ($1,'productImport',$2,$3,$4,$5,$5)"#,
    )
    .bind(run_id)
    .bind(operation_status)
    .bind("Import verified Product Master CSV")
    .bind(operation_result)
    .bind(created_at)
    .execute(&mut *transaction)
    .await?;
    if queued {
        // This is the complete durable job payload. In particular, it must
        // never grow to include CSV text, normalized product facts, filenames,
        // prices, or encrypted source data.
        sqlx::query(
            r#"INSERT INTO jobs
               (id,job_type,status,payload,available_at,created_at,updated_at)
               VALUES ($1,'productImport','queued',$2,$3,$3,$3)"#,
        )
        .bind(run_id)
        .bind(product_import_job_payload(run_id))
        .bind(created_at)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,'productMaster.import.accept','productImport',$3,NULL,$4,$5,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(audit_actor)
    .bind(run_id)
    .bind(json!({
        "operationId": run_id,
        "environment": environment,
        "sourceChecksum": parsed.result.checksum,
        "mappingVersion": parsed.result.mapping_version,
        "totalRows": parsed.result.total_rows,
        "validRows": parsed.result.valid_rows,
        "malformedRows": parsed.result.malformed_rows,
        "authorityAdmission": authority_decision.audit_label(),
        "queued": queued
    }))
    .bind("Accept encrypted verified Product Master staging and durable promotion job.")
    .bind(Uuid::new_v4())
    .bind(created_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(StagedProductImport {
        operation_id: run_id,
        result: parsed.result,
        queued,
    })
}

fn product_import_job_payload(import_run_id: Uuid) -> Value {
    json!({"importRunId": import_run_id})
}

async fn persist_staged_row(
    transaction: &mut Transaction<'_, Postgres>,
    parsed: &ParsedProductImport,
    row: &ImportedProductRow,
) -> Result<(), ApiError> {
    let encrypted = row.confidential_payload.as_deref().ok_or_else(|| {
        ApiError::internal("A valid Product Master row is missing encrypted staging data.")
    })?;
    if encrypted.len() < 28 {
        return Err(ApiError::internal(
            "Encrypted Product Master row is invalid.",
        ));
    }
    let nonce = &encrypted[..12];
    let tag = &encrypted[encrypted.len() - 16..];
    let ciphertext = &encrypted[12..encrypted.len() - 16];
    sqlx::query(
        r#"INSERT INTO product_import_private_staging
           (id,import_run_id,source_record_id,source_row_number,ciphertext,encryption_algorithm,
            encryption_key_id,nonce,authentication_tag,checksum,status,created_at,expires_at)
           VALUES ($1,$2,$3,$4,$5,'AES-256-GCM','environment-v1',$6,$7,$8,
                   'validated',$9,$10)"#,
    )
    .bind(Uuid::new_v4())
    .bind(parsed.result.id)
    .bind(&row.stable_id)
    .bind(row.row_number)
    .bind(ciphertext)
    .bind(nonce)
    .bind(tag)
    .bind(format!("{:x}", Sha256::digest(encrypted)))
    .bind(parsed.result.created_at)
    .bind(parsed.result.created_at + Duration::days(30))
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO product_import_normalized_records
           (id,import_run_id,source_record_id,normalized_payload,validation_status,created_at)
           VALUES ($1,$2,$3,$4,'valid',$5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(parsed.result.id)
    .bind(&row.stable_id)
    .bind(&row.normalized_payload)
    .bind(parsed.result.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_import_findings(
    transaction: &mut Transaction<'_, Postgres>,
    parsed: &ParsedProductImport,
) -> Result<(), ApiError> {
    for error in &parsed.result.errors {
        sqlx::query(
            r#"INSERT INTO product_import_errors
               (id,import_run_id,source_record_id,severity,error_code,field_path,message,details,created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(Uuid::new_v4())
        .bind(parsed.result.id)
        .bind(error.stable_id.as_deref())
        .bind(&error.severity)
        .bind(&error.code)
        .bind(error.field_name.as_deref())
        .bind(&error.detail)
        .bind(json!({"rowNumber": error.row_number}))
        .bind(parsed.result.created_at)
        .execute(&mut **transaction)
        .await?;
    }
    for missing in &parsed.result.missing_assets {
        sqlx::query(
            r#"INSERT INTO product_import_missing_assets
               (id,import_run_id,source_record_id,asset_type,source_reference,
                resolution_status,created_at)
               VALUES ($1,$2,$3,$4,$5,'missing',$6)"#,
        )
        .bind(Uuid::new_v4())
        .bind(parsed.result.id)
        .bind(&missing.stable_id)
        .bind(&missing.asset_type)
        .bind(&missing.source_reference)
        .bind(parsed.result.created_at)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn persist_source_resolution_audit(
    transaction: &mut Transaction<'_, Postgres>,
    parsed: &ParsedProductImport,
    audit_actor: &str,
    actor_id: Option<Uuid>,
) -> Result<(), ApiError> {
    if !parsed
        .rows
        .iter()
        .any(|row| row.stable_id == "B23E280H128-102-B0")
    {
        return Ok(());
    }
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,'product.sourceResolution','productImport',$3,NULL,$4,$5,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(audit_actor)
    .bind(parsed.result.id)
    .bind(json!({
        "stableId": "B23E280H128-102-B0",
        "acceptedAuthority": "verifiedProductMasterCsv",
        "verifiedCsvChecksum": parsed.result.checksum,
        "mappingVersion": parsed.result.mapping_version,
        "acceptedBy": actor_id,
        "excludedSourceClass": "interactionDemo"
    }))
    .bind("The confirmed Product Master CSV is the accepted source; interaction demos supplied no product facts.")
    .bind(Uuid::new_v4())
    .bind(parsed.result.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Promote every normalized row from one durable import run into immutable
/// draft product revisions. The transaction is all-or-nothing. A worker crash
/// after this commit is harmless: the completed run is returned on retry and
/// no second product revision is created.
pub async fn promote_staged_product_import(
    pool: &PgPool,
    import_run_id: Uuid,
    approved: Option<&ApprovedProductMaster>,
) -> Result<ProductImportResult, ApiError> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("verifiedCsv:promote:{import_run_id}"))
        .execute(&mut *transaction)
        .await?;
    let run = sqlx::query(
        r#"SELECT environment,source_checksum,mapping_version,status,created_by,
                  records_received,records_valid
           FROM product_import_runs
           WHERE id=$1 AND data_origin='verifiedCsv'
           FOR UPDATE"#,
    )
    .bind(import_run_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Product import run was not found."))?;
    let status: String = run.try_get("status")?;
    let environment: String = run.try_get("environment")?;
    let import_checksum: String = run.try_get("source_checksum")?;
    let mapping_version: String = run.try_get("mapping_version")?;
    let records_received: i64 = run.try_get("records_received")?;
    let records_valid: i64 = run.try_get("records_valid")?;
    verify_product_master_authority(
        &environment,
        approved,
        &import_checksum,
        &mapping_version,
        records_valid,
        records_received.saturating_sub(records_valid),
    )?;
    if status == "completed" {
        transaction.rollback().await?;
        return load_product_import_result(pool, import_run_id)
            .await?
            .ok_or_else(|| ApiError::service_unavailable("Stored product import is incomplete."));
    }
    if status == "failed" || status == "cancelled" {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "A failed or cancelled Product Master import cannot be promoted.",
        ));
    }
    let actor_id: Option<Uuid> = run.try_get("created_by")?;
    let now = Utc::now();
    sqlx::query("UPDATE product_import_runs SET status='validating' WHERE id=$1")
        .bind(import_run_id)
        .execute(&mut *transaction)
        .await?;
    let rows = sqlx::query(
        r#"SELECT normalized.source_record_id,private.source_row_number,
                  normalized.normalized_payload
           FROM product_import_normalized_records AS normalized
           JOIN product_import_private_staging AS private
             ON private.import_run_id=normalized.import_run_id
            AND private.source_record_id=normalized.source_record_id
           WHERE normalized.import_run_id=$1 AND normalized.validation_status='valid'
           ORDER BY private.source_row_number,normalized.source_record_id"#,
    )
    .bind(import_run_id)
    .fetch_all(&mut *transaction)
    .await?;
    if rows.is_empty() {
        return Err(ApiError::conflict(
            "Product import has no validated rows to promote.",
        ));
    }
    for stored in rows {
        let row = ImportedProductRow {
            row_number: stored.try_get("source_row_number")?,
            stable_id: stored.try_get("source_record_id")?,
            normalized_payload: stored.try_get("normalized_payload")?,
            confidential_payload: None,
        };
        promote_verified_csv_product(
            &mut transaction,
            import_run_id,
            &import_checksum,
            &row,
            actor_id,
            now,
        )
        .await?;
        sqlx::query(
            r#"UPDATE product_import_private_staging
               SET status='promoted',processed_at=$3
               WHERE import_run_id=$1 AND source_record_id=$2"#,
        )
        .bind(import_run_id)
        .bind(&row.stable_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query("UPDATE product_import_runs SET status='completed',completed_at=$2 WHERE id=$1")
        .bind(import_run_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    load_product_import_result(pool, import_run_id)
        .await?
        .ok_or_else(|| ApiError::service_unavailable("Stored product import is incomplete."))
}

/// Re-open a terminally failed import only while its authenticated private
/// staging rows still exist. This is called in the same transaction that
/// resets the durable job and operation, preventing a queued job from pointing
/// at an import run the worker must reject.
pub async fn reset_staged_product_import_for_retry(
    transaction: &mut Transaction<'_, Postgres>,
    import_run_id: Uuid,
) -> Result<(), ApiError> {
    let reset = sqlx::query(
        r#"UPDATE product_import_runs AS import
           SET status='readyToPublish',completed_at=NULL
           WHERE import.id=$1 AND import.status='failed'
             AND EXISTS (
                 SELECT 1 FROM product_import_normalized_records AS normalized
                 JOIN product_import_private_staging AS private
                   ON private.import_run_id=normalized.import_run_id
                  AND private.source_record_id=normalized.source_record_id
                 WHERE normalized.import_run_id=import.id
                   AND normalized.validation_status='valid'
                   AND private.status='validated'
                   AND private.expires_at > now()
             )"#,
    )
    .bind(import_run_id)
    .execute(&mut **transaction)
    .await?;
    if reset.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "Product import staging is missing, expired, or not retryable.",
        ));
    }
    Ok(())
}

async fn promote_verified_csv_product(
    transaction: &mut Transaction<'_, Postgres>,
    import_run_id: Uuid,
    import_checksum: &str,
    row: &ImportedProductRow,
    actor_id: Option<Uuid>,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    let existing = sqlx::query(
        "SELECT id,current_revision,published_revision,data_origin FROM products WHERE stable_id=$1 FOR UPDATE",
    )
    .bind(&row.stable_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let (product_id, revision, published_revision): (Uuid, i64, Option<i64>) = match existing {
        Some(existing) => {
            let origin: String = existing.try_get("data_origin")?;
            if origin != "verifiedCsv" {
                return Err(ApiError::conflict(format!(
                    "Product `{}` is owned by {origin} and cannot be overwritten by verified CSV.",
                    row.stable_id
                )));
            }
            (
                existing.try_get("id")?,
                existing.try_get::<i64, _>("current_revision")? + 1,
                existing.try_get("published_revision")?,
            )
        }
        None => (Uuid::new_v4(), 1_i64, None),
    };
    let mut payload = row
        .normalized_payload
        .as_object()
        .cloned()
        .ok_or_else(|| ApiError::internal("Normalized product payload is not an object."))?;
    payload.insert("id".into(), json!(product_id));
    payload.insert("sourceSnapshotId".into(), json!(Uuid::nil()));
    payload.insert(
        "sourceRevision".into(),
        json!(format!("csv:{import_checksum}")),
    );
    payload.insert("currentRevision".into(), json!(revision));
    payload.insert("publishedRevision".into(), json!(published_revision));
    payload.insert("status".into(), json!("draft"));
    payload.insert("indexable".into(), json!(false));
    payload.insert("updatedAt".into(), json!(now));
    payload.insert("dataOrigin".into(), json!("verifiedCsv"));
    let payload = Value::Object(payload);
    let model = payload.get("model").and_then(Value::as_str);
    let slug = required_payload_text(&payload, "slug")?;
    let locale = required_payload_text(&payload, "locale")?;
    let family = required_payload_text(&payload, "family")?;
    let title = required_payload_text(&payload, "title")?;
    let summary = payload.get("summary").and_then(Value::as_str);

    if revision == 1 {
        sqlx::query(
            r#"INSERT INTO products
               (id,stable_id,model,slug,locale,family,source_snapshot_id,source_revision,
                status,current_revision,published_revision,indexable,payload,updated_at,
                data_origin,product_import_run_id)
               VALUES ($1,$2,$3,$4,$5,$6,NULL,$7,'draft',1,NULL,false,$8,$9,
                       'verifiedCsv',$10)"#,
        )
        .bind(product_id)
        .bind(&row.stable_id)
        .bind(model)
        .bind(slug)
        .bind(locale)
        .bind(family)
        .bind(format!("csv:{import_checksum}"))
        .bind(&payload)
        .bind(now)
        .bind(import_run_id)
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            r#"UPDATE products SET model=$2,slug=$3,locale=$4,family=$5,
                      source_snapshot_id=NULL,source_revision=$6,status='draft',
                      current_revision=$7,indexable=false,payload=$8,updated_at=$9,
                      data_origin='verifiedCsv',product_import_run_id=$10
               WHERE id=$1"#,
        )
        .bind(product_id)
        .bind(model)
        .bind(slug)
        .bind(locale)
        .bind(family)
        .bind(format!("csv:{import_checksum}"))
        .bind(revision)
        .bind(&payload)
        .bind(now)
        .bind(import_run_id)
        .execute(&mut **transaction)
        .await?;
    }
    sqlx::query(
        r#"INSERT INTO product_revisions
           (product_id,revision,source_snapshot_id,payload,created_at,data_origin,product_import_run_id)
           VALUES ($1,$2,NULL,$3,$4,'verifiedCsv',$5)"#,
    )
    .bind(product_id)
    .bind(revision)
    .bind(&payload)
    .bind(now)
    .bind(import_run_id)
    .execute(&mut **transaction)
    .await?;
    // Product Master updates advance only the immutable facts revision. The
    // portal-owned website presentation is initialized once and then remains
    // independent across every later CSV/Feishu import.
    if revision == 1 {
        let presentation_actor = actor_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "productImportWorker".into());
        let presentation_seo = json!({
            "title": null,
            "description": null,
            "canonicalPath": null,
            "indexable": false
        });
        sqlx::query(
            r#"INSERT INTO product_presentation_working
               (product_id,locale,current_revision,published_revision,slug,title,summary,
                content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                updated_by,updated_at)
               VALUES ($1,$2,1,NULL,$3,$4,$5,'{}'::jsonb,$6,'draft',false,false,
                       'verifiedCsv',$7,$8)"#,
        )
        .bind(product_id)
        .bind(locale)
        .bind(slug)
        .bind(title)
        .bind(summary)
        .bind(&presentation_seo)
        .bind(&presentation_actor)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO product_presentation_revisions
               (product_id,locale,revision,source_product_revision,slug,title,summary,
                content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                created_by,created_at)
               VALUES ($1,$2,1,1,$3,$4,$5,'{}'::jsonb,$6,'draft',false,false,
                       'verifiedCsv',$7,$8)"#,
        )
        .bind(product_id)
        .bind(locale)
        .bind(slug)
        .bind(title)
        .bind(summary)
        .bind(&presentation_seo)
        .bind(&presentation_actor)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }
    project_product_facts(
        transaction,
        product_id,
        revision,
        &payload,
        &format!("verified-csv:{import_checksum}:{}", row.stable_id),
    )
    .await?;
    Ok(())
}

fn required_payload_text<'a>(payload: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::internal(format!("Normalized product `{key}` is missing.")))
}

pub async fn load_product_import_result(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<ProductImportResult>, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,source_checksum,mapping_version,status,records_received,records_valid,
                  created_at FROM product_import_runs WHERE id=$1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let error_rows = sqlx::query(
        r#"SELECT source_record_id,severity,error_code,field_path,message,details
           FROM product_import_errors WHERE import_run_id=$1
           ORDER BY created_at,id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let errors = error_rows
        .into_iter()
        .map(|row| {
            let details: Value = row.try_get("details")?;
            Ok(ProductImportError {
                row_number: details
                    .get("rowNumber")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                stable_id: row.try_get("source_record_id")?,
                field_name: row.try_get("field_path")?,
                severity: row.try_get("severity")?,
                code: row.try_get("error_code")?,
                detail: row.try_get("message")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let missing_rows = sqlx::query(
        r#"SELECT source_record_id,asset_type,source_reference
           FROM product_import_missing_assets
           WHERE import_run_id=$1 AND resolution_status='missing' ORDER BY created_at,id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let missing_assets = missing_rows
        .into_iter()
        .map(|row| {
            Ok(MissingAssetReference {
                stable_id: row.try_get("source_record_id")?,
                asset_type: row.try_get("asset_type")?,
                source_reference: row.try_get("source_reference")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let total_rows: i64 = row.try_get("records_received")?;
    let valid_rows: i64 = row.try_get("records_valid")?;
    Ok(Some(ProductImportResult {
        id: row.try_get("id")?,
        checksum: row.try_get("source_checksum")?,
        mapping_version: row.try_get("mapping_version")?,
        status: row.try_get("status")?,
        total_rows,
        valid_rows,
        malformed_rows: total_rows.saturating_sub(valid_rows),
        errors,
        missing_assets,
        reused: false,
        created_at: row.try_get("created_at")?,
    }))
}

pub fn parse_product_master(
    csv: &str,
    mapping_version: &str,
    key: Option<&ProductStagingEncryptionKey>,
) -> Result<ParsedProductImport, ApiError> {
    if csv.is_empty() || csv.len() > MAX_CSV_BYTES {
        return Err(ApiError::bad_request(
            "csv must contain between 1 byte and 16 MiB.",
        ));
    }
    if mapping_version.trim().is_empty() || mapping_version.len() > 100 {
        return Err(ApiError::bad_request(
            "mappingVersion must contain 1 to 100 characters.",
        ));
    }

    let checksum = format!("{:x}", Sha256::digest(csv.as_bytes()));
    let records = parse_csv(csv)?;
    if records.len() < 2 {
        return Err(ApiError::bad_request(
            "csv must contain a header row and at least one product row.",
        ));
    }
    if records.len() > MAX_ROWS + 1 {
        return Err(ApiError::bad_request("csv contains too many rows."));
    }

    let header = records[0]
        .1
        .iter()
        .map(|value| normalize_header(value))
        .collect::<Vec<_>>();
    let header_index = header
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_empty())
        .map(|(index, value)| (value.clone(), index))
        .collect::<HashMap<_, _>>();
    if header_index.is_empty() {
        return Err(ApiError::bad_request("csv header row is empty."));
    }

    let stable_index = find_header(
        &header_index,
        &["stableid", "productid", "sku", "id", "文本"],
    )
    .ok_or_else(|| ApiError::bad_request("csv is missing the stableId column."))?;
    let family_index = find_header(
        &header_index,
        &[
            "family",
            "productfamily",
            "category",
            "taxonomy",
            "一级分类product1",
            "一级分类",
        ],
    )
    .ok_or_else(|| ApiError::bad_request("csv is missing the family column."))?;
    let model_index = find_header(
        &header_index,
        &["model", "modelnumber", "partnumber", "产品型号model"],
    );
    let title_index = find_header(&header_index, &["title", "name", "productname"]);
    let slug_index = find_header(&header_index, &["slug", "urlslug"]);
    let locale_index = find_header(&header_index, &["locale", "language"]);
    let subtype_index = find_header(
        &header_index,
        &["subtype", "type", "二级类product2", "二级类"],
    );
    let motor_index = find_header(&header_index, &["motortechnology", "motortype", "motor"]);
    let summary_index = find_header(&header_index, &["summary", "description"]);
    let asset_indexes = header
        .iter()
        .enumerate()
        .filter(|(_, name)| is_asset_header(name))
        .map(|(index, name)| (index, asset_type(name).to_owned()))
        .collect::<Vec<_>>();

    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut missing_assets = Vec::new();
    let mut seen = HashSet::new();
    for (row_number, record) in records.into_iter().skip(1) {
        // A physical blank line is harmless, but an explicitly present CSV
        // row made of delimiters is a malformed source record and must remain
        // visible in the import report (the verified master has one such row).
        if record.len() == 1 && record[0].trim().is_empty() {
            continue;
        }
        let stable_id = field(&record, Some(stable_index)).trim().to_owned();
        let mut row_errors = Vec::new();
        if stable_id.is_empty() || stable_id.len() > 200 {
            row_errors.push(import_error(
                row_number,
                none_if_empty(&stable_id),
                "stableId",
                "invalidStableId",
                "stableId must contain 1 to 200 characters.",
            ));
        } else if !seen.insert(stable_id.to_lowercase()) {
            row_errors.push(import_error(
                row_number,
                Some(&stable_id),
                "stableId",
                "duplicateStableId",
                "stableId is duplicated in this import.",
            ));
        }
        if !row_errors.is_empty() {
            errors.extend(row_errors);
            continue;
        }
        let raw_family = field(&record, Some(family_index));
        let family = normalize_family(raw_family);
        if family.is_none() {
            row_errors.push(import_error(
                row_number,
                none_if_empty(&stable_id),
                "family",
                "invalidFamily",
                "family must map to Centrifugal, Axial, Cross-flow, Inline Duct, or Motors.",
            ));
        }
        if !row_errors.is_empty() {
            errors.extend(row_errors);
            continue;
        }

        let model = non_empty(field(&record, model_index));
        let title = non_empty(field(&record, title_index))
            .or_else(|| model.clone())
            .unwrap_or_else(|| stable_id.clone());
        let supplied_slug = non_empty(field(&record, slug_index));
        let slug = supplied_slug
            .as_deref()
            .map(slugify)
            .filter(|slug| !slug.is_empty())
            .unwrap_or_else(|| slugify(model.as_deref().unwrap_or(&stable_id)));
        if slug.is_empty() {
            errors.push(import_error(
                row_number,
                Some(&stable_id),
                "slug",
                "invalidSlug",
                "A URL-safe slug could not be derived.",
            ));
            continue;
        }

        let locale = non_empty(field(&record, locale_index)).unwrap_or_else(|| "en".into());
        if !valid_locale(&locale) {
            errors.push(import_error(
                row_number,
                Some(&stable_id),
                "locale",
                "invalidLocale",
                "locale must be a simple BCP 47 language tag.",
            ));
            continue;
        }

        for (index, kind) in &asset_indexes {
            for reference in split_asset_references(field(&record, Some(*index))) {
                missing_assets.push(MissingAssetReference {
                    stable_id: stable_id.clone(),
                    asset_type: kind.clone(),
                    source_reference: reference,
                });
            }
        }

        let key = key.ok_or_else(|| {
            ApiError::service_unavailable(
                "Product staging encryption is not configured; source rows cannot be imported.",
            )
        })?;
        let source_row = header
            .iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    name.clone(),
                    Value::String(field(&record, Some(index)).to_owned()),
                )
            })
            .collect::<Map<_, _>>();
        let confidential_payload = Some(encrypt_confidential(
            key,
            format!("{mapping_version}:{checksum}:{row_number}:{stable_id}").as_bytes(),
            &serde_json::to_vec(&source_row)
                .map_err(|_| ApiError::internal("Private product staging serialization failed."))?,
        )?);

        // This projection is intentionally allow-listed. Price and every
        // noise-related column remain absent even when present in the source.
        let mut source_fields = Map::new();
        for (index, name) in header.iter().enumerate() {
            if is_safe_optional_header(name) {
                let value = field(&record, Some(index)).trim();
                if !value.is_empty() {
                    source_fields.insert(name.clone(), Value::String(value.to_owned()));
                }
            }
        }
        let (specifications, warnings) =
            normalized_specifications(&header_index, &record, &checksum, row_number, &stable_id);
        errors.extend(warnings);
        let operating_conditions = normalized_operating_conditions(&header_index, &record);
        let subtype = non_empty(field(&record, subtype_index));
        let motor_technology = normalize_motor_technology(field(&record, motor_index))
            .or_else(|| normalize_motor_technology(subtype.as_deref().unwrap_or_default()));
        let normalized_payload = json!({
            "stableId": stable_id,
            "model": model,
            "slug": slug,
            "locale": locale,
            "family": family.expect("family was validated"),
            "subtype": subtype,
            "motorTechnology": motor_technology,
            "title": title,
            "summary": non_empty(field(&record, summary_index)),
            "specifications": specifications,
            "operatingConditions": operating_conditions,
            "performanceCurves": [],
            "sourceRevision": format!("csv:{checksum}"),
            "mappingVersion": mapping_version,
            "sourceFields": source_fields,
        });
        rows.push(ImportedProductRow {
            row_number,
            stable_id,
            normalized_payload,
            confidential_payload,
        });
    }

    let fatal_rows = errors
        .iter()
        .filter(|error| error.severity == "error")
        .map(|error| error.row_number)
        .collect::<HashSet<_>>();
    let total_rows = rows.len() as i64 + fatal_rows.len() as i64;
    let malformed_rows = fatal_rows.len() as i64;
    let status = if rows.is_empty() {
        "failed"
    } else if errors.is_empty() {
        "validated"
    } else {
        "validatedWithErrors"
    };
    let created_at = Utc::now();
    Ok(ParsedProductImport {
        result: ProductImportResult {
            id: Uuid::new_v4(),
            checksum,
            mapping_version: mapping_version.to_owned(),
            status: status.into(),
            total_rows,
            valid_rows: rows.len() as i64,
            malformed_rows,
            errors,
            missing_assets,
            reused: false,
            created_at,
        },
        rows,
    })
}

fn parse_csv(csv: &str) -> Result<Vec<(i32, Vec<String>)>, ApiError> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = csv.trim_start_matches('\u{feff}').chars().peekable();
    let mut physical_line = 1_i32;
    let mut record_line = 1_i32;
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => record.push(std::mem::take(&mut field)),
            '\r' if !quoted => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push((record_line, std::mem::take(&mut record)));
                physical_line += 1;
                record_line = physical_line;
            }
            '\n' if !quoted => {
                record.push(std::mem::take(&mut field));
                records.push((record_line, std::mem::take(&mut record)));
                physical_line += 1;
                record_line = physical_line;
            }
            '\n' => {
                field.push('\n');
                physical_line += 1;
            }
            value => field.push(value),
        }
    }
    if quoted {
        return Err(ApiError::bad_request(format!(
            "csv contains an unterminated quoted field beginning near row {record_line}."
        )));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push((record_line, record));
    }
    Ok(records)
}

fn encrypt_confidential(
    key: &ProductStagingEncryptionKey,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ApiError> {
    let unbound = UnboundKey::new(&aead::AES_256_GCM, key.as_bytes())
        .map_err(|_| ApiError::internal("Product staging encryption setup failed."))?;
    let key = LessSafeKey::new(unbound);
    let mut nonce_bytes = [0_u8; 12];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| ApiError::internal("Secure random generation failed."))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut ciphertext = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, Aad::from(aad), &mut ciphertext)
        .map_err(|_| ApiError::internal("Product staging encryption failed."))?;
    let mut nonce_prefixed = nonce_bytes.to_vec();
    nonce_prefixed.extend(ciphertext);
    Ok(nonce_prefixed)
}

/// Decrypt a private source row and return only explicitly allow-listed
/// pricing fields. Callers must separately enforce `product.pricing.read`,
/// emit an audit record, and return `Cache-Control: no-store`.
///
/// The authenticated-data tuple binds ciphertext to the exact import mapping,
/// file checksum, physical row, and stable product id. Moving ciphertext
/// between rows or import runs therefore fails closed.
pub fn decrypt_private_pricing(
    key: &ProductStagingEncryptionKey,
    envelope: PrivatePricingEnvelope<'_>,
) -> Result<BTreeMap<String, String>, ApiError> {
    let nonce: [u8; 12] = envelope.nonce.try_into().map_err(|_| {
        ApiError::service_unavailable("Private Product Master staging metadata is invalid.")
    })?;
    if envelope.authentication_tag.len() != 16 {
        return Err(ApiError::service_unavailable(
            "Private Product Master staging metadata is invalid.",
        ));
    }
    let unbound = UnboundKey::new(&aead::AES_256_GCM, key.as_bytes())
        .map_err(|_| ApiError::internal("Product staging decryption setup failed."))?;
    let key = LessSafeKey::new(unbound);
    let mut ciphertext_and_tag =
        Vec::with_capacity(envelope.ciphertext.len() + envelope.authentication_tag.len());
    ciphertext_and_tag.extend_from_slice(envelope.ciphertext);
    ciphertext_and_tag.extend_from_slice(envelope.authentication_tag);
    let aad = format!(
        "{}:{}:{}:{}",
        envelope.mapping_version, envelope.checksum, envelope.source_row_number, envelope.stable_id
    );
    let plaintext = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad.as_bytes()),
            &mut ciphertext_and_tag,
        )
        .map_err(|_| {
            ApiError::service_unavailable("Private Product Master staging authentication failed.")
        })?;
    let source = serde_json::from_slice::<Map<String, Value>>(plaintext)
        .map_err(|_| ApiError::service_unavailable("Private Product Master staging is invalid."))?;
    Ok(source
        .into_iter()
        .filter_map(|(name, value)| {
            (is_private_pricing_header(envelope.mapping_version, &name))
                .then(|| value.as_str().map(|value| (name, value.to_owned())))
                .flatten()
        })
        .collect())
}

fn is_private_pricing_header(mapping_version: &str, name: &str) -> bool {
    match mapping_version {
        // Exact normalized headers from the user-confirmed Product Master.
        // Adding another commercial field requires an explicit mapping-version
        // change; substring matching could accidentally expose unrelated notes.
        "airtek-basic-v1" => matches!(
            name,
            "样品报价sample" | "100500pcs" | "5001000pcs" | "10005000pcs" | "5000pcs"
        ),
        // Minimal synthetic contract mapping used only by unit tests.
        "v1" => matches!(name, "price" | "cost" | "currency"),
        _ => false,
    }
}

fn find_header(index: &HashMap<String, usize>, aliases: &[&str]) -> Option<usize> {
    aliases.iter().find_map(|alias| index.get(*alias).copied())
}

fn field(record: &[String], index: Option<usize>) -> &str {
    index
        .and_then(|index| record.get(index))
        .map(String::as_str)
        .unwrap_or_default()
}

fn normalize_header(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('\u{feff}')
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_family(value: &str) -> Option<&'static str> {
    let compact = normalize_header(value);
    match compact.as_str() {
        "centrifugal"
        | "centrifugalfan"
        | "centrifugalfans"
        | "plugfan"
        | "离心"
        | "离心风机"
        | "离心风机centrifugalfans" => Some("centrifugal"),
        "axial" | "axialfan" | "轴流" | "轴流风机" => Some("axial"),
        "crossflow" | "crossflowfan" | "贯流" | "横流" | "贯流风机" => Some("crossFlow"),
        "inlineduct" | "inline" | "duct" | "ductfan" | "inlineductfan" | "管道" | "管道风机" => {
            Some("inlineDuct")
        }
        "motor" | "motors" | "电机" => Some("motors"),
        _ => None,
    }
}

fn normalize_motor_technology(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let compact = normalize_header(value);
    let normalized = match compact.as_str() {
        "ec" | "electronicallycommutated" => "EC",
        "ac" => "AC",
        "dc" => "DC",
        "bldc" => "BLDC",
        _ if compact.starts_with("ec") => "EC",
        _ if compact.starts_with("ac") => "AC",
        _ => value,
    };
    Some(normalized.to_owned())
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    slug
}

fn valid_locale(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn none_if_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn normalized_specifications(
    header_index: &HashMap<String, usize>,
    record: &[String],
    checksum: &str,
    row_number: i32,
    stable_id: &str,
) -> (Vec<Value>, Vec<ProductImportError>) {
    let frequency = value_for(
        header_index,
        record,
        &["frequency", "频率frequency", "频率"],
    );
    let source_reference = format!("verified-csv:{checksum}:row-{row_number}:{stable_id}");
    type SpecificationDefinition = (
        &'static str,
        &'static str,
        &'static [&'static str],
        Option<&'static str>,
        bool,
    );
    let definitions: &[SpecificationDefinition] = &[
        (
            "voltage",
            "Voltage",
            &["voltage", "电压voltage", "电压"],
            Some("V"),
            true,
        ),
        (
            "frequency",
            "Frequency",
            &["frequency", "频率frequency", "频率"],
            Some("Hz"),
            false,
        ),
        (
            "diameter",
            "Diameter",
            &["diameter", "直径diameter", "直径"],
            Some("mm"),
            false,
        ),
        (
            "current",
            "Current",
            &["current", "电流current", "电流"],
            Some("A"),
            true,
        ),
        (
            "speed",
            "Speed",
            &["speed", "转速speed", "转速"],
            Some("rpm"),
            true,
        ),
        (
            "airflow",
            "Air Flow",
            &["airflow", "风量airflow", "风量"],
            Some("m³/h"),
            true,
        ),
        (
            "pressure",
            "Air Pressure",
            &["airpressure", "风压airpressure", "风压"],
            Some("Pa"),
            true,
        ),
        (
            "power",
            "Power",
            &["power", "功率power", "功率"],
            Some("W"),
            true,
        ),
        (
            "material",
            "Material",
            &["material", "风轮材质material", "风轮材质"],
            None,
            false,
        ),
        (
            "dimension",
            "Product Dimension",
            &["dimension", "产品尺寸dimension", "产品尺寸"],
            None,
            false,
        ),
        (
            "packageData",
            "Package Data",
            &["packagedata", "包装数据packagedata", "包装数据"],
            None,
            false,
        ),
        (
            "protectionClass",
            "Protection Class",
            &["protectionclass", "防护等级protectionclass", "防护等级"],
            None,
            false,
        ),
        (
            "insulationClass",
            "Insulation Class",
            &["insulationclass", "绝缘等级insulationclass", "绝缘等级"],
            None,
            false,
        ),
    ];
    let mut specifications = Vec::new();
    let mut warnings = Vec::new();
    for (key, label, aliases, unit, frequency_scoped) in definitions {
        let Some(raw) = value_for(header_index, record, aliases) else {
            continue;
        };
        let values = split_paired(&raw);
        let frequencies = frequency.as_deref().map(split_paired).unwrap_or_default();
        if *frequency_scoped && values.len() > 1 && values.len() == frequencies.len() {
            for (index, value) in values.into_iter().enumerate() {
                specifications.push(json!({
                    "key": format!("{key}@{}Hz", frequencies[index]),
                    "label": label,
                    "value": value,
                    "unit": unit,
                    "operatingCondition": format!("{} Hz", frequencies[index]),
                    "state": "verified",
                    "sourceReference": source_reference,
                }));
            }
        } else {
            let pending =
                *frequency_scoped && values.len() > 1 && values.len() != frequencies.len();
            if pending {
                warnings.push(ProductImportError {
                    row_number,
                    stable_id: Some(stable_id.to_owned()),
                    field_name: Some((*key).to_owned()),
                    severity: "warning".into(),
                    code: "unpairedOperatingCondition".into(),
                    detail: "Multiple values do not map one-to-one to frequency values; the raw value is retained pending verification.".into(),
                });
            }
            specifications.push(json!({
                "key": key,
                "label": label,
                "value": raw,
                "unit": unit,
                "operatingCondition": if *frequency_scoped {
                    frequency.as_ref().map(|value| format!("Frequency: {value} Hz"))
                } else { None },
                "state": if pending { "pendingVerification" } else { "verified" },
                "sourceReference": source_reference,
            }));
        }
    }
    (specifications, warnings)
}

fn normalized_operating_conditions(
    header_index: &HashMap<String, usize>,
    record: &[String],
) -> Vec<Value> {
    value_for(
        header_index,
        record,
        &["frequency", "频率frequency", "频率"],
    )
    .map(|value| {
        split_paired(&value)
            .into_iter()
            .enumerate()
            .map(|(index, frequency)| {
                json!({
                    "key": format!("frequency-{}", index + 1),
                    "frequencyHz": frequency,
                    "label": format!("{frequency} Hz"),
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

fn value_for(
    header_index: &HashMap<String, usize>,
    record: &[String],
    aliases: &[&str],
) -> Option<String> {
    non_empty(field(record, find_header(header_index, aliases)))
}

fn split_paired(value: &str) -> Vec<String> {
    value
        .split('/')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn is_asset_header(name: &str) -> bool {
    name.contains("asset")
        || name.contains("image")
        || name.contains("drawing")
        || name.contains("cad")
        || name.contains("certificate")
        || name.contains("datasheet")
        || name.contains("曲线图")
        || name.contains("图纸")
        || name.contains("规格书")
}

fn asset_type(name: &str) -> &'static str {
    if name.contains("cad")
        || name.contains("drawing")
        || name.contains("2d图纸")
        || name.contains("3d图纸")
    {
        "cad"
    } else if name.contains("certificate") {
        "certificate"
    } else if name.contains("datasheet") || name.contains("规格书") {
        "datasheet"
    } else {
        "image"
    }
}

fn split_asset_references(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split([';', '|'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn is_safe_optional_header(name: &str) -> bool {
    // Do not ever place confidential price, source noise measurements, or
    // asset locator fields in the public candidate payload.
    !name.contains("price")
        && !name.contains("cost")
        && !name.contains("noise")
        && !name.contains("sound")
        && !name.contains("db")
        && !is_asset_header(name)
        && matches!(
            name,
            "voltage"
                | "frequency"
                | "frequencyhz"
                | "power"
                | "powerunit"
                | "airflow"
                | "airflowunit"
                | "pressure"
                | "pressureunit"
                | "speed"
                | "speedunit"
                | "diameter"
                | "diameterunit"
                | "certifications"
                | "iprating"
        )
}

fn import_error(
    row_number: i32,
    stable_id: Option<&str>,
    field_name: &str,
    code: &str,
    detail: &str,
) -> ProductImportError {
    ProductImportError {
        row_number,
        stable_id: stable_id.map(str::to_owned),
        field_name: Some(field_name.into()),
        severity: "error".into(),
        code: code.into(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApprovedProductMaster, Config};

    fn authority_for(parsed: &ParsedProductImport) -> ApprovedProductMaster {
        ApprovedProductMaster {
            sha256: parsed.result.checksum.clone(),
            mapping_version: parsed.result.mapping_version.clone(),
            expected_valid_rows: parsed.result.valid_rows,
            expected_error_rows: parsed.result.malformed_rows,
        }
    }

    #[test]
    fn import_keeps_valid_rows_reports_bad_rows_and_excludes_noise_and_price() {
        let csv = concat!(
            "stable_id,model,family,title,price,currency,noise_db,image\n",
            "p-1,B23E280H128-102-B0,Centrifugal,Verified model,12.50,USD,55,missing.jpg\n",
            "p-2,,unknown,Bad row,,,,\n",
            "p-3,M3,Axial,Another verified model,,,,\n"
        );
        let config = Config::for_test();
        let parsed = parse_product_master(
            csv,
            "airtek-basic-v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .expect("valid import");
        assert_eq!(parsed.result.total_rows, 3);
        assert_eq!(parsed.result.valid_rows, 2);
        assert_eq!(parsed.result.malformed_rows, 1);
        assert_eq!(parsed.result.missing_assets.len(), 1);
        assert!(parsed.rows[0].confidential_payload.is_some());
        let serialized = parsed.rows[0].normalized_payload.to_string();
        assert!(!serialized.contains("12.50"));
        assert!(!serialized.to_lowercase().contains("noise"));
        assert!(!serialized.contains("missing.jpg"));
        assert_eq!(parsed.rows[0].normalized_payload["family"], "centrifugal");
    }

    #[test]
    fn production_rejects_unapproved_modified_truncated_and_wrong_mapping_sources() {
        let config = Config::for_test();
        let key = config.product_staging_encryption_key.as_ref();
        let approved_csv = concat!(
            "stable_id,model,family,title\n",
            "p-1,M1,Centrifugal,Approved model\n"
        );
        let approved = parse_product_master(approved_csv, "airtek-basic-v1", key).unwrap();
        let authority = authority_for(&approved);
        assert_eq!(
            verify_product_master_authority(
                "production",
                Some(&authority),
                &approved.result.checksum,
                &approved.result.mapping_version,
                approved.result.valid_rows,
                approved.result.malformed_rows,
            )
            .unwrap(),
            ProductMasterAuthorityDecision::ProductionApproved
        );

        for (label, csv, mapping) in [
            (
                "arbitrary",
                "stable_id,model,family,title\np-2,X1,Axial,Other model\n",
                "airtek-basic-v1",
            ),
            (
                "single-byte modification",
                "stable_id,model,family,title\np-1,N1,Centrifugal,Approved model\n",
                "airtek-basic-v1",
            ),
            (
                "truncation",
                "stable_id,model,family,title\np-1,M1,Centrifugal,Approved mod",
                "airtek-basic-v1",
            ),
            ("wrong mapping", approved_csv, "airtek-basic-v2"),
        ] {
            let parsed = parse_product_master(csv, mapping, key).unwrap();
            assert!(
                verify_product_master_authority(
                    "production",
                    Some(&authority),
                    &parsed.result.checksum,
                    &parsed.result.mapping_version,
                    parsed.result.valid_rows,
                    parsed.result.malformed_rows,
                )
                .is_err(),
                "{label} must not enter production staging"
            );
        }
        assert!(verify_product_master_authority(
            "production",
            None,
            &approved.result.checksum,
            &approved.result.mapping_version,
            approved.result.valid_rows,
            approved.result.malformed_rows,
        )
        .is_err());
    }

    #[test]
    fn development_recognizes_the_registered_source_without_requiring_it() {
        let config = Config::for_test();
        let parsed = parse_product_master(
            "stable_id,model,family\np-1,M1,Centrifugal\n",
            "airtek-basic-v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .unwrap();
        let authority = authority_for(&parsed);
        assert_eq!(
            verify_product_master_authority(
                "development",
                Some(&authority),
                &parsed.result.checksum,
                &parsed.result.mapping_version,
                parsed.result.valid_rows,
                parsed.result.malformed_rows,
            )
            .unwrap(),
            ProductMasterAuthorityDecision::ConfiguredApprovedDevelopment
        );
        assert!(verify_product_master_authority(
            "development",
            Some(&authority),
            &"0".repeat(64),
            &parsed.result.mapping_version,
            parsed.result.valid_rows,
            parsed.result.malformed_rows,
        )
        .is_err());
    }

    #[test]
    fn confidential_source_round_trips_only_with_row_bound_aad() {
        let csv = concat!(
            "stable_id,model,family,price,price_notes,noise_db\n",
            "p-1,M1,Centrifugal,12.50,never-expose-this-note,55\n"
        );
        let config = Config::for_test();
        let key = config.product_staging_encryption_key.as_ref().unwrap();
        let parsed = parse_product_master(csv, "v1", Some(key)).unwrap();
        let row = &parsed.rows[0];
        let sealed = row.confidential_payload.as_ref().unwrap();
        let nonce = &sealed[..12];
        let ciphertext = &sealed[12..sealed.len() - 16];
        let tag = &sealed[sealed.len() - 16..];
        let pricing = decrypt_private_pricing(
            key,
            PrivatePricingEnvelope {
                mapping_version: "v1",
                checksum: &parsed.result.checksum,
                source_row_number: row.row_number,
                stable_id: &row.stable_id,
                nonce,
                ciphertext,
                authentication_tag: tag,
            },
        )
        .unwrap();
        assert_eq!(pricing.get("price").map(String::as_str), Some("12.50"));
        assert!(!pricing.contains_key("pricenotes"));
        assert!(!pricing.contains_key("noisedb"));
        assert!(decrypt_private_pricing(
            key,
            PrivatePricingEnvelope {
                mapping_version: "v1",
                checksum: &parsed.result.checksum,
                source_row_number: 999,
                stable_id: &row.stable_id,
                nonce,
                ciphertext,
                authentication_tag: tag,
            },
        )
        .is_err());
    }

    #[test]
    fn durable_job_payload_contains_only_the_import_run_reference() {
        let id = Uuid::new_v4();
        let payload = product_import_job_payload(id);
        assert_eq!(payload, json!({"importRunId": id}));
        let serialized = payload.to_string().to_ascii_lowercase();
        assert!(!serialized.contains("csv"));
        assert!(!serialized.contains("price"));
        assert!(!serialized.contains("ciphertext"));
    }

    #[test]
    fn confirmed_master_pricing_headers_are_private_and_allowlisted_on_read() {
        let csv = concat!(
            "文本,一级分类_Product #1,样品报价（sample）,100～500 pcs,500～1000pcs,1000～5000pcs,≥5000pcs,噪声(Noise Level)\n",
            "M1,离心风机 Centrifugal fans,sample,band-1,band-2,band-3,band-4,55\n"
        );
        let config = Config::for_test();
        let key = config.product_staging_encryption_key.as_ref().unwrap();
        let parsed = parse_product_master(csv, "airtek-basic-v1", Some(key)).unwrap();
        let row = &parsed.rows[0];
        let sealed = row.confidential_payload.as_ref().unwrap();
        let pricing = decrypt_private_pricing(
            key,
            PrivatePricingEnvelope {
                mapping_version: "airtek-basic-v1",
                checksum: &parsed.result.checksum,
                source_row_number: row.row_number,
                stable_id: &row.stable_id,
                nonce: &sealed[..12],
                ciphertext: &sealed[12..sealed.len() - 16],
                authentication_tag: &sealed[sealed.len() - 16..],
            },
        )
        .unwrap();
        assert_eq!(pricing.len(), 5);
        assert_eq!(pricing["样品报价sample"], "sample");
        assert_eq!(pricing["100500pcs"], "band-1");
        assert_eq!(pricing["5001000pcs"], "band-2");
        assert_eq!(pricing["10005000pcs"], "band-3");
        assert_eq!(pricing["5000pcs"], "band-4");
        assert!(pricing.values().all(|value| value != "55"));
    }

    #[test]
    fn csv_parser_supports_commas_quotes_and_newlines() {
        let rows =
            parse_csv("a,b\n1,\"two, three\"\n2,\"line one\nline two\"\n").expect("valid CSV");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].1[1], "two, three");
        assert_eq!(rows[2].1[1], "line one\nline two");
    }

    #[test]
    fn duplicate_stable_ids_are_malformed() {
        let config = Config::for_test();
        let parsed = parse_product_master(
            "stableId,family\np-1,axial\np-1,axial\n",
            "v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .unwrap();
        assert_eq!(parsed.result.valid_rows, 1);
        assert_eq!(parsed.result.malformed_rows, 1);
        assert_eq!(parsed.result.errors[0].code, "duplicateStableId");
    }

    #[test]
    fn verified_product_master_contract_is_370_valid_and_5_malformed_when_fixture_is_provided() {
        let Ok(path) = std::env::var("AIRTEK_PRODUCT_MASTER_TEST_CSV") else {
            return;
        };
        let csv = std::fs::read_to_string(path).expect("verified Product Master CSV is readable");
        let config = Config::for_test();
        let parsed = parse_product_master(
            &csv,
            "airtek-basic-v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .expect("verified Product Master is accepted");
        assert_eq!(
            parsed.result.checksum,
            "e3b944d5d979c72d963ba353416ef452f9dac0bf63182bb09fc6b1201c043800"
        );
        assert_eq!(parsed.result.total_rows, 375);
        assert_eq!(parsed.result.valid_rows, 370);
        assert_eq!(parsed.result.malformed_rows, 5);
        assert_eq!(
            parsed
                .result
                .errors
                .iter()
                .filter(|error| error.severity == "error")
                .count(),
            5
        );
        assert!(parsed.rows.iter().all(|row| {
            let payload = row.normalized_payload.to_string().to_lowercase();
            !payload.contains("noise") && !payload.contains("sample") && !payload.contains("price")
        }));
    }
}
