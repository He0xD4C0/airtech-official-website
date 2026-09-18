use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, models::SyncRunKind, state::AppState};

use super::{
    seal_cursor, DiscoveredSource, FeishuResumeCursor, FeishuTableMapping, VersionedFeishuMapping,
};

pub(super) fn runtime_table_mapping(
    mapping: &VersionedFeishuMapping,
    source: &DiscoveredSource,
) -> Result<FeishuTableMapping, ApiError> {
    let validated = super::validate_mapping(mapping.clone(), std::slice::from_ref(source))?;
    validated
        .tables
        .get(&source.source.table_id)
        .cloned()
        .ok_or_else(|| ApiError::conflict("The Feishu table is absent from this mapping version."))
}

pub(super) async fn staging_exists(
    state: &AppState,
    run_id: Uuid,
    source_record_id: &str,
) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM staging_records WHERE sync_run_id=$1 AND source_record_id=$2)",
    )
    .bind(run_id)
    .bind(source_record_id)
    .fetch_one(&state.pool)
    .await?)
}

pub(super) async fn next_source_row_number(
    state: &AppState,
    run_id: Uuid,
) -> Result<i32, ApiError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM product_import_private_staging WHERE import_run_id=$1",
    )
    .bind(run_id)
    .fetch_one(&state.pool)
    .await?;
    i32::try_from(count + 2)
        .map_err(|_| ApiError::conflict("The Feishu run contains too many records."))
}

pub(super) async fn persist_cursor(
    state: &AppState,
    run_id: Uuid,
    key: &crate::config::ProductStagingEncryptionKey,
    cursor: &FeishuResumeCursor,
    assets_copied: u64,
    assets_reused: u64,
    assets_failed: u64,
) -> Result<(), ApiError> {
    let sealed = seal_cursor(key, run_id, cursor)?;
    sqlx::query(
        r#"UPDATE sync_runs SET resume_cursor=$2,assets_copied=$3,assets_reused=$4,
                  assets_failed=$5 WHERE id=$1"#,
    )
    .bind(run_id)
    .bind(sealed)
    .bind(i64::try_from(assets_copied).unwrap_or(i64::MAX))
    .bind(i64::try_from(assets_reused).unwrap_or(i64::MAX))
    .bind(i64::try_from(assets_failed).unwrap_or(i64::MAX))
    .execute(&state.pool)
    .await?;
    Ok(())
}

pub(super) async fn increment_table_failure(
    state: &AppState,
    run_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("UPDATE sync_runs SET records_failed=records_failed+1 WHERE id=$1")
        .bind(run_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub(super) async fn report_suspected_missing(
    state: &AppState,
    run_id: Uuid,
    table_ids: &[String],
) -> Result<(), ApiError> {
    for table_id in table_ids {
        let prefix = format!("fs.{table_id}.%");
        let missing = sqlx::query_scalar::<_, String>(
            r#"SELECT product.stable_id FROM products product
               WHERE product.data_origin='feishu' AND product.stable_id LIKE $1
                 AND NOT EXISTS (
                   SELECT 1 FROM staging_records staging
                   WHERE staging.sync_run_id=$2 AND staging.source_record_id=product.stable_id
                 )
               ORDER BY product.stable_id"#,
        )
        .bind(prefix)
        .bind(run_id)
        .fetch_all(&state.pool)
        .await?;
        for stable_id in missing {
            sqlx::query(
                r#"INSERT INTO product_import_errors
                   (id,import_run_id,source_record_id,severity,error_code,message,details,created_at)
                   VALUES ($1,$2,$3,'warning','suspectedMissing',$4,'{}'::jsonb,now())"#,
            )
            .bind(Uuid::new_v4())
            .bind(run_id)
            .bind(&stable_id)
            .bind("The record was absent from a full reconciliation; no automatic archive was applied.")
            .execute(&state.pool)
            .await?;
        }
    }
    Ok(())
}

pub(super) async fn finalize_run(
    state: &AppState,
    run_id: Uuid,
    run_kind: SyncRunKind,
    assets_copied: u64,
    assets_reused: u64,
    assets_failed: u64,
) -> Result<(), ApiError> {
    let counts = sqlx::query(
        r#"SELECT count(*) AS seen,
                  count(*) FILTER (WHERE validation_status='valid') AS valid,
                  count(*) FILTER (WHERE validation_status='invalid') AS invalid,
                  COALESCE(sum(jsonb_array_length(source.source_payload->'attachments')),0) AS assets_seen
           FROM staging_records staging
           JOIN source_snapshots source ON source.id=staging.source_snapshot_id
           WHERE staging.sync_run_id=$1"#,
    )
    .bind(run_id)
    .fetch_one(&state.pool)
    .await?;
    let applied: i64 =
        sqlx::query_scalar("SELECT count(*) FROM feishu_sync_changes WHERE sync_run_id=$1")
            .bind(run_id)
            .fetch_one(&state.pool)
            .await?;
    let run_level_errors: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM product_import_errors
           WHERE import_run_id=$1 AND severity='error' AND source_record_id IS NULL"#,
    )
    .bind(run_id)
    .fetch_one(&state.pool)
    .await?;
    let invalid: i64 = counts.try_get("invalid")?;
    let failed = invalid + run_level_errors;
    let status = if failed > 0 || assets_failed > 0 {
        "completedWithErrors"
    } else {
        "completed"
    };
    sqlx::query(
        r#"UPDATE sync_runs SET status=$2,resume_cursor=NULL,records_seen=$3,records_valid=$4,
                  records_applied=$5,records_failed=$6,assets_seen=$7,assets_copied=$8,
                  assets_reused=$9,assets_failed=$10,completed_at=now(),
                  payload=jsonb_build_object('status',$2)
           WHERE id=$1"#,
    )
    .bind(run_id)
    .bind(status)
    .bind(counts.try_get::<i64, _>("seen")?)
    .bind(counts.try_get::<i64, _>("valid")?)
    .bind(applied)
    .bind(failed)
    .bind(counts.try_get::<i64, _>("assets_seen")?)
    .bind(i64::try_from(assets_copied).unwrap_or(i64::MAX))
    .bind(i64::try_from(assets_reused).unwrap_or(i64::MAX))
    .bind(i64::try_from(assets_failed).unwrap_or(i64::MAX))
    .execute(&state.pool)
    .await?;
    sqlx::query(
        r#"UPDATE product_import_runs SET status='completed',records_received=$2,
                  records_valid=$3,error_count=$4,completed_at=now() WHERE id=$1"#,
    )
    .bind(run_id)
    .bind(counts.try_get::<i64, _>("seen")?)
    .bind(counts.try_get::<i64, _>("valid")?)
    .bind(failed)
    .execute(&state.pool)
    .await?;
    let column = match run_kind {
        SyncRunKind::Incremental => "last_incremental_at",
        SyncRunKind::Full => "last_full_at",
    };
    let query = format!(
        "UPDATE feishu_connector_settings SET {column}=now() WHERE connector_id=(SELECT connector_id FROM sync_runs WHERE id=$1)"
    );
    sqlx::query(&query)
        .bind(run_id)
        .execute(&state.pool)
        .await?;
    Ok(())
}
