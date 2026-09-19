use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{error::ApiError, services::product_import::encrypt_confidential, state::AppState};

use super::NormalizedFeishuRecord;

#[allow(clippy::too_many_arguments)]
pub async fn stage_invalid_record(
    state: &AppState,
    connector_id: Uuid,
    sync_run_id: Uuid,
    import_run_id: Uuid,
    mapping_version: &str,
    import_checksum: &str,
    source_row_number: i32,
    record: &NormalizedFeishuRecord,
) -> Result<(), ApiError> {
    let stable_id = record
        .normalized_payload
        .get("stableId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&record.source_record_id);
    let key = state
        .config
        .product_staging_encryption_key
        .as_ref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Product staging encryption must be configured for Feishu synchronization.",
            )
        })?;
    let plaintext = serde_json::to_vec(&record.confidential_payload)
        .map_err(|_| ApiError::internal("Feishu private staging serialization failed."))?;
    let aad = format!("{mapping_version}:{import_checksum}:{source_row_number}:{stable_id}");
    let encrypted = encrypt_confidential(key, aad.as_bytes(), &plaintext)?;
    if encrypted.len() < 28 {
        return Err(ApiError::internal("Encrypted Feishu staging is invalid."));
    }
    let mut transaction = state.pool.begin().await?;
    let validation_results = record
        .issues
        .iter()
        .chain(record.warnings.iter())
        .cloned()
        .collect::<Vec<_>>();
    let snapshot_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO source_snapshots
           (id,connector_id,sync_run_id,source_record_id,source_revision,checksum,source_payload)
           VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
    )
    .bind(snapshot_id)
    .bind(connector_id)
    .bind(sync_run_id)
    .bind(&record.source_record_id)
    .bind(&record.source_revision)
    .bind(&record.confidential_checksum)
    .bind(&record.snapshot_payload)
    .execute(&mut *transaction)
    .await?;
    let staging_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO staging_records
           (id,sync_run_id,source_snapshot_id,source_record_id,validation_status,
            normalized_payload,validation_errors)
           VALUES ($1,$2,$3,$4,'invalid',$5,$6)"#,
    )
    .bind(staging_id)
    .bind(sync_run_id)
    .bind(snapshot_id)
    .bind(&record.source_record_id)
    .bind(&record.normalized_payload)
    .bind(json!(validation_results))
    .execute(&mut *transaction)
    .await?;
    let nonce = &encrypted[..12];
    let tag = &encrypted[encrypted.len() - 16..];
    let ciphertext = &encrypted[12..encrypted.len() - 16];
    let private_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO product_import_private_staging
           (id,import_run_id,source_record_id,source_row_number,ciphertext,encryption_algorithm,
            encryption_key_id,nonce,authentication_tag,checksum,status,promoted_staging_record_id,
            created_at,expires_at,processed_at)
           VALUES ($1,$2,$3,$4,$5,'AES-256-GCM','environment-v1',$6,$7,$8,
                   'rejected',$9,now(),now()+interval '30 days',now())"#,
    )
    .bind(private_id)
    .bind(import_run_id)
    .bind(&record.source_record_id)
    .bind(source_row_number)
    .bind(ciphertext)
    .bind(nonce)
    .bind(tag)
    .bind(format!("{:x}", Sha256::digest(&encrypted)))
    .bind(staging_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO product_import_normalized_records
           (id,import_run_id,source_record_id,normalized_payload,validation_status)
           VALUES ($1,$2,$3,$4,'invalid')"#,
    )
    .bind(Uuid::new_v4())
    .bind(import_run_id)
    .bind(&record.source_record_id)
    .bind(&record.normalized_payload)
    .execute(&mut *transaction)
    .await?;
    for issue in &record.issues {
        sqlx::query(
            r#"INSERT INTO product_import_errors
               (id,import_run_id,private_staging_id,source_record_id,severity,error_code,
                field_path,message,details,created_at)
               VALUES ($1,$2,$3,$4,'error',$5,$6,$7,'{}'::jsonb,now())"#,
        )
        .bind(Uuid::new_v4())
        .bind(import_run_id)
        .bind(private_id)
        .bind(&record.source_record_id)
        .bind(&issue.code)
        .bind(&issue.field_path)
        .bind(&issue.detail)
        .execute(&mut *transaction)
        .await?;
    }
    for warning in &record.warnings {
        sqlx::query(
            r#"INSERT INTO product_import_errors
               (id,import_run_id,private_staging_id,source_record_id,severity,error_code,
                field_path,message,details,created_at)
               VALUES ($1,$2,$3,$4,'warning',$5,$6,$7,'{}'::jsonb,now())"#,
        )
        .bind(Uuid::new_v4())
        .bind(import_run_id)
        .bind(private_id)
        .bind(&record.source_record_id)
        .bind(&warning.code)
        .bind(&warning.field_path)
        .bind(&warning.detail)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn record_sync_error(
    state: &AppState,
    import_run_id: Uuid,
    source_record_id: Option<&str>,
    code: &str,
    message: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO product_import_errors
           (id,import_run_id,source_record_id,severity,error_code,message,details,created_at)
           VALUES ($1,$2,$3,'error',$4,$5,'{}'::jsonb,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(import_run_id)
    .bind(source_record_id)
    .bind(code)
    .bind(message)
    .execute(&state.pool)
    .await?;
    Ok(())
}
