use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{FeishuSyncError, FeishuSyncRunDetail, SyncRun, SyncRunKind, SyncRunStatus},
    services::object_storage_settings,
    state::AppState,
};

use super::get_feishu_settings;

pub async fn queue_sync_run(
    state: &AppState,
    run_kind: SyncRunKind,
    actor: &str,
) -> Result<SyncRun, ApiError> {
    if !state.feishu_client.configured() {
        return Err(ApiError::service_unavailable(
            "Feishu credentials are not configured.",
        ));
    }
    if state.config.product_staging_encryption_key.is_none() {
        return Err(ApiError::service_unavailable(
            "Product staging encryption is not configured.",
        ));
    }
    object_storage_settings::active_storage(state).await?;
    let settings = get_feishu_settings(state).await?;
    let now = Utc::now();
    let id = Uuid::new_v4();
    let run = SyncRun {
        id,
        connector_id: Some(settings.connector_id),
        source: "feishu".into(),
        dry_run: false,
        run_kind,
        mapping_version: settings.mapping_version.clone(),
        status: SyncRunStatus::Queued,
        resume_cursor: None,
        records_seen: 0,
        records_valid: 0,
        conflict_count: 0,
        records_applied: 0,
        records_failed: 0,
        assets_seen: 0,
        assets_copied: 0,
        assets_reused: 0,
        assets_failed: 0,
        started_at: now,
        completed_at: None,
        error: None,
    };
    let payload = serde_json::to_value(&run)
        .map_err(|_| ApiError::internal("Feishu run serialization failed."))?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("feishu:queue:{}", settings.connector_id))
        .execute(&mut *transaction)
        .await?;
    let active = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM sync_runs WHERE connector_id=$1
           AND status IN ('queued','fetching','validating','readyToPublish')
           ORDER BY started_at LIMIT 1"#,
    )
    .bind(settings.connector_id)
    .fetch_optional(&mut *transaction)
    .await?;
    if active.is_some() {
        return Err(ApiError::conflict(
            "This Feishu connector already has an active synchronization run.",
        ));
    }
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,connector_id,source,dry_run,run_kind,mapping_version,status,resume_cursor,
            records_seen,records_valid,conflict_count,records_applied,records_failed,
            assets_seen,assets_copied,assets_reused,assets_failed,started_at,payload)
           VALUES ($1,$2,'feishu',false,$3,$4,'queued',NULL,0,0,0,0,0,0,0,0,0,$5,$6)"#,
    )
    .bind(id)
    .bind(settings.connector_id)
    .bind(run_kind.label())
    .bind(&settings.mapping_version)
    .bind(now)
    .bind(payload)
    .execute(&mut *transaction)
    .await?;
    let import_checksum = format!("feishu-run:{id}");
    sqlx::query(
        r#"INSERT INTO product_import_runs
           (id,sync_run_id,connector_id,environment,data_origin,dry_run,status,
            mapping_version,source_checksum,records_received,records_valid,error_count,
            started_at,created_at)
           VALUES ($1,$1,$2,$3,'feishu',false,'queued',$4,$5,0,0,0,$6,$6)"#,
    )
    .bind(id)
    .bind(settings.connector_id)
    .bind(state.environment_label())
    .bind(&settings.mapping_version)
    .bind(&import_checksum)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO operation_runs(id,kind,status,reason,result,created_at,updated_at)
           VALUES ($1,'feishuSync','queued',$2,NULL,$3,$3)"#,
    )
    .bind(id)
    .bind(format!("{} Feishu Product Master sync", run_kind.label()))
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO jobs
           (id,job_type,status,payload,available_at,created_at,updated_at)
           VALUES ($1,'feishuSync','queued',$2,$3,$3,$3)"#,
    )
    .bind(id)
    .bind(json!({"syncRunId": id}))
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,
            request_id,occurred_at)
           VALUES ($1,$2,'feishu.sync.queue','feishuSync',$3,NULL,$4,$5,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(id)
    .bind(json!({"runKind": run_kind.label(), "mappingVersion": settings.mapping_version}))
    .bind("Queue one-way automatic Product Master synchronization")
    .bind(Uuid::new_v4())
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(run)
}

pub async fn load_sync_run(state: &AppState, id: Uuid) -> Result<SyncRun, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,connector_id,source,dry_run,run_kind,mapping_version,status,
                  resume_cursor,records_seen,records_valid,conflict_count,records_applied,
                  records_failed,assets_seen,assets_copied,assets_reused,assets_failed,
                  started_at,completed_at,payload
           FROM sync_runs WHERE id=$1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Feishu sync run was not found."))?;
    decode_sync_run(&row)
}

pub async fn load_sync_run_detail(
    state: &AppState,
    id: Uuid,
) -> Result<FeishuSyncRunDetail, ApiError> {
    let run = load_sync_run(state, id).await?;
    let rows = sqlx::query(
        r#"SELECT source_record_id,error_code,field_path,message,created_at
           FROM product_import_errors WHERE import_run_id=$1
           ORDER BY created_at,id"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let errors = rows
        .iter()
        .map(|row| {
            Ok(FeishuSyncError {
                source_record_id: row.try_get("source_record_id")?,
                code: row.try_get("error_code")?,
                field_path: row.try_get("field_path")?,
                message: row.try_get("message")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(FeishuSyncRunDetail { run, errors })
}

pub fn decode_sync_run(row: &PgRow) -> Result<SyncRun, ApiError> {
    let status: String = row.try_get("status")?;
    let run_kind: String = row.try_get("run_kind")?;
    let payload: Value = row.try_get("payload")?;
    Ok(SyncRun {
        id: row.try_get("id")?,
        connector_id: row.try_get("connector_id")?,
        source: row.try_get("source")?,
        dry_run: row.try_get("dry_run")?,
        run_kind: decode_enum(&run_kind, "Feishu run kind")?,
        mapping_version: row.try_get("mapping_version")?,
        status: decode_enum(&status, "Feishu run status")?,
        resume_cursor: row.try_get("resume_cursor")?,
        records_seen: counter(row, "records_seen")?,
        records_valid: counter(row, "records_valid")?,
        conflict_count: counter(row, "conflict_count")?,
        records_applied: counter(row, "records_applied")?,
        records_failed: counter(row, "records_failed")?,
        assets_seen: counter(row, "assets_seen")?,
        assets_copied: counter(row, "assets_copied")?,
        assets_reused: counter(row, "assets_reused")?,
        assets_failed: counter(row, "assets_failed")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        error: payload
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn counter(row: &PgRow, name: &str) -> Result<u64, ApiError> {
    let value: i64 = row.try_get(name)?;
    u64::try_from(value).map_err(|_| ApiError::service_unavailable("Stored sync count is invalid."))
}

fn decode_enum<T: serde::de::DeserializeOwned>(value: &str, label: &str) -> Result<T, ApiError> {
    serde_json::from_value(Value::String(value.into())).map_err(|error| {
        tracing::error!(%error, label, "stored Feishu enum is invalid");
        ApiError::service_unavailable(format!("Stored {label} is invalid."))
    })
}
