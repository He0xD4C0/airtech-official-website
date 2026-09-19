use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        FeishuRunTableResult, FeishuSettings, FeishuSyncError, FeishuSyncRunDetail,
        FeishuSyncTrigger, SyncRun, SyncRunStatus,
    },
    services::object_storage_settings,
    state::AppState,
};

use super::{get_feishu_settings_in_transaction, CONNECTOR_ID};

pub async fn queue_full_sync(
    state: &AppState,
    trigger: FeishuSyncTrigger,
    actor: &str,
) -> Result<SyncRun, ApiError> {
    match try_queue_full_sync(state, trigger, actor).await? {
        Some(run) => Ok(run),
        None => {
            let active = active_work_id(state).await?;
            let mut error = ApiError::conflict("This Feishu connector already has active work.");
            if let Some(active_run_id) = active {
                error = error.with_active_run_id(active_run_id);
            }
            Err(error)
        }
    }
}

pub async fn try_queue_full_sync(
    state: &AppState,
    trigger: FeishuSyncTrigger,
    actor: &str,
) -> Result<Option<SyncRun>, ApiError> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("feishu:queue:{CONNECTOR_ID}"))
        .execute(&mut *transaction)
        .await?;
    let settings = get_feishu_settings_in_transaction(&mut transaction).await?;
    ensure_queue_prerequisites(state, &settings).await?;
    let run =
        queue_full_sync_in_transaction(&mut transaction, state, &settings, trigger, actor).await?;
    if run.is_none() {
        transaction.rollback().await?;
        return Ok(None);
    }
    transaction.commit().await?;
    Ok(run)
}

pub(super) async fn queue_full_sync_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    state: &AppState,
    settings: &FeishuSettings,
    trigger: FeishuSyncTrigger,
    actor: &str,
) -> Result<Option<SyncRun>, ApiError> {
    let sources = settings
        .sources
        .iter()
        .filter(|source| source.enabled)
        .cloned()
        .collect::<Vec<_>>();
    if !settings.enabled {
        return Err(ApiError::conflict("The Feishu connector is disabled."));
    }
    if settings.tested_connection_revision != Some(settings.connection_revision) {
        return Err(ApiError::conflict(
            "Run a successful connection test for the current Feishu configuration first.",
        ));
    }
    if sources.is_empty() {
        return Err(ApiError::conflict(
            "Enable at least one Feishu product table before synchronizing.",
        ));
    }
    let now = Utc::now();
    let id = Uuid::new_v4();
    let run = SyncRun {
        id,
        connector_id: Some(settings.connector_id),
        source: "feishu".into(),
        dry_run: false,
        trigger,
        settings_revision: settings.revision,
        sources: sources.clone(),
        mapping_version: settings.mapping_version.clone(),
        status: SyncRunStatus::Queued,
        resume_cursor: None,
        records_seen: 0,
        records_valid: 0,
        records_applied: 0,
        records_failed: 0,
        records_deleted: 0,
        assets_seen: 0,
        assets_copied: 0,
        assets_reused: 0,
        assets_failed: 0,
        started_at: now,
        completed_at: None,
        error: None,
    };
    let source_config = serde_json::to_value(&sources)
        .map_err(|_| ApiError::internal("Feishu source snapshot serialization failed."))?;
    let payload = serde_json::to_value(&run)
        .map_err(|_| ApiError::internal("Feishu run serialization failed."))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("feishu:queue:{}", settings.connector_id))
        .execute(&mut **transaction)
        .await?;
    if active_work_in_transaction(transaction, settings.connector_id).await? {
        return Ok(None);
    }
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,connector_id,source,dry_run,trigger,settings_revision,source_config,
            mapping_version,status,resume_cursor,records_seen,records_valid,
            records_applied,records_failed,records_deleted,assets_seen,assets_copied,
            assets_reused,assets_failed,started_at,payload)
           VALUES ($1,$2,'feishu',false,$3,$4,$5,$6,'queued',NULL,
                   0,0,0,0,0,0,0,0,0,$7,$8)"#,
    )
    .bind(id)
    .bind(settings.connector_id)
    .bind(trigger.label())
    .bind(settings.revision)
    .bind(&source_config)
    .bind(&settings.mapping_version)
    .bind(now)
    .bind(payload)
    .execute(&mut **transaction)
    .await?;
    for source in &sources {
        sqlx::query(
            r#"INSERT INTO feishu_run_table_results
               (sync_run_id,wiki_token,table_id,source_name,status)
               VALUES ($1,$2,$3,$4,'pending')"#,
        )
        .bind(id)
        .bind(&source.wiki_token)
        .bind(&source.table_id)
        .bind(&source.name)
        .execute(&mut **transaction)
        .await?;
    }
    insert_related_run_rows(
        transaction,
        state,
        &run,
        &settings.mapping_version,
        trigger,
        actor,
        now,
    )
    .await?;
    Ok(Some(run))
}

async fn ensure_queue_prerequisites(
    state: &AppState,
    settings: &FeishuSettings,
) -> Result<(), ApiError> {
    if !settings.secret_configured {
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
    Ok(())
}

async fn active_work_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    connector_id: Uuid,
) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM sync_runs WHERE connector_id=$1
             AND status IN ('queued','fetching','validating','readyToPublish'))
           OR EXISTS(SELECT 1 FROM jobs WHERE job_type='feishuSourcePurge'
             AND status IN ('queued','running') AND payload->>'connectorId'=$1::text)"#,
    )
    .bind(connector_id)
    .fetch_one(&mut **transaction)
    .await?)
}

async fn active_work_id(state: &AppState) -> Result<Option<Uuid>, ApiError> {
    Ok(sqlx::query_scalar(
        r#"SELECT id FROM (
             SELECT id,created_at FROM jobs WHERE job_type='feishuSourcePurge'
               AND status IN ('queued','running') AND payload->>'connectorId'=$1::text
             UNION ALL
             SELECT id,started_at FROM sync_runs WHERE connector_id=$1
               AND status IN ('queued','fetching','validating','readyToPublish')
           ) active ORDER BY created_at LIMIT 1"#,
    )
    .bind(CONNECTOR_ID)
    .fetch_optional(&state.pool)
    .await?)
}

#[allow(clippy::too_many_arguments)]
async fn insert_related_run_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    state: &AppState,
    run: &SyncRun,
    mapping_version: &str,
    trigger: FeishuSyncTrigger,
    actor: &str,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    let import_checksum = format!("feishu-run:{}", run.id);
    sqlx::query(
        r#"INSERT INTO product_import_runs
           (id,sync_run_id,connector_id,environment,data_origin,dry_run,status,
            mapping_version,source_checksum,records_received,records_valid,error_count,
            started_at,created_at)
           VALUES ($1,$1,$2,$3,'feishu',false,'queued',$4,$5,0,0,0,$6,$6)"#,
    )
    .bind(run.id)
    .bind(run.connector_id)
    .bind(state.environment_label())
    .bind(mapping_version)
    .bind(import_checksum)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO operation_runs(id,kind,status,reason,result,created_at,updated_at)
           VALUES ($1,'feishuSync','queued',$2,NULL,$3,$3)"#,
    )
    .bind(run.id)
    .bind(format!(
        "{} full Feishu Product Master sync",
        trigger.label()
    ))
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO jobs
           (id,job_type,status,payload,connector_id,available_at,created_at,updated_at)
           VALUES ($1,'feishuSync','queued',$2,$3,$4,$4,$4)"#,
    )
    .bind(run.id)
    .bind(json!({"syncRunId": run.id}))
    .bind(run.connector_id)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,
            request_id,occurred_at)
           VALUES ($1,$2,'feishu.sync.queue','feishuSync',$3,NULL,$4,$5,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(run.id)
    .bind(
        json!({"trigger": trigger.label(), "mappingVersion": mapping_version,
                 "settingsRevision": run.settings_revision}),
    )
    .bind("Queue one-way full Product Master synchronization")
    .bind(Uuid::new_v4())
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn load_sync_run(state: &AppState, id: Uuid) -> Result<SyncRun, ApiError> {
    let row = sqlx::query(&format!(
        "{} WHERE id=$1 AND source='feishu' AND connector_id=$2",
        sync_run_select()
    ))
    .bind(id)
    .bind(CONNECTOR_ID)
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
    let table_rows = sqlx::query(
        r#"SELECT sync_run_id,wiki_token,table_id,source_name,status,records_seen,records_applied,
                  records_failed,records_deleted,assets_seen,assets_copied,assets_reused,
                  assets_failed,error,completed_at
           FROM feishu_run_table_results WHERE sync_run_id=$1 ORDER BY wiki_token,table_id"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let tables = table_rows
        .iter()
        .map(decode_table_result)
        .collect::<Result<Vec<_>, _>>()?;
    let rows = sqlx::query(
        r#"SELECT source_record_id,severity,error_code,field_path,message,created_at
           FROM product_import_errors WHERE import_run_id=$1 ORDER BY created_at,id"#,
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let errors = rows
        .iter()
        .map(|row| {
            Ok(FeishuSyncError {
                source_record_id: row.try_get("source_record_id")?,
                severity: row.try_get("severity")?,
                code: row.try_get("error_code")?,
                field_path: row.try_get("field_path")?,
                message: row.try_get("message")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(FeishuSyncRunDetail {
        run,
        tables,
        errors,
    })
}

pub(crate) fn sync_run_select() -> &'static str {
    r#"SELECT id,connector_id,source,dry_run,trigger,settings_revision,source_config,
              mapping_version,status,resume_cursor,records_seen,records_valid,
              records_applied,records_failed,records_deleted,assets_seen,assets_copied,
              assets_reused,assets_failed,started_at,completed_at,payload FROM sync_runs"#
}

pub fn decode_sync_run(row: &PgRow) -> Result<SyncRun, ApiError> {
    let status: String = row.try_get("status")?;
    let trigger: String = row.try_get("trigger")?;
    let payload: Value = row.try_get("payload")?;
    let source_config: Value = row.try_get("source_config")?;
    Ok(SyncRun {
        id: row.try_get("id")?,
        connector_id: row.try_get("connector_id")?,
        source: row.try_get("source")?,
        dry_run: row.try_get("dry_run")?,
        trigger: decode_enum(&trigger, "Feishu trigger")?,
        settings_revision: row.try_get("settings_revision")?,
        sources: serde_json::from_value(source_config).map_err(|error| {
            tracing::error!(%error, "stored Feishu run sources are invalid");
            ApiError::service_unavailable("Stored Feishu run sources are invalid.")
        })?,
        mapping_version: row.try_get("mapping_version")?,
        status: decode_enum(&status, "Feishu run status")?,
        resume_cursor: row.try_get("resume_cursor")?,
        records_seen: counter(row, "records_seen")?,
        records_valid: counter(row, "records_valid")?,
        records_applied: counter(row, "records_applied")?,
        records_failed: counter(row, "records_failed")?,
        records_deleted: counter(row, "records_deleted")?,
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

fn decode_table_result(row: &PgRow) -> Result<FeishuRunTableResult, ApiError> {
    Ok(FeishuRunTableResult {
        sync_run_id: row.try_get("sync_run_id")?,
        wiki_token: row.try_get("wiki_token")?,
        table_id: row.try_get("table_id")?,
        source_name: row.try_get("source_name")?,
        status: row.try_get("status")?,
        records_seen: counter(row, "records_seen")?,
        records_applied: counter(row, "records_applied")?,
        records_failed: counter(row, "records_failed")?,
        records_deleted: counter(row, "records_deleted")?,
        assets_seen: counter(row, "assets_seen")?,
        assets_copied: counter(row, "assets_copied")?,
        assets_reused: counter(row, "assets_reused")?,
        assets_failed: counter(row, "assets_failed")?,
        error: row.try_get("error")?,
        completed_at: row.try_get("completed_at")?,
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
