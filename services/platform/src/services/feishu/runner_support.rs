use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

use super::{
    seal_cursor, DiscoveredSource, FeishuResumeCursor, FeishuTableMapping, VersionedFeishuMapping,
};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TableAssetCounts {
    pub(super) seen: u64,
    pub(super) copied: u64,
    pub(super) reused: u64,
    pub(super) failed: u64,
}

pub(super) async fn load_table_asset_counts(
    state: &AppState,
    run_id: Uuid,
    wiki_token: &str,
    table_id: &str,
) -> Result<TableAssetCounts, ApiError> {
    let row = sqlx::query(
        r#"SELECT assets_seen,assets_copied,assets_reused,assets_failed
           FROM feishu_run_table_results
           WHERE sync_run_id=$1 AND wiki_token=$2 AND table_id=$3"#,
    )
    .bind(run_id)
    .bind(wiki_token)
    .bind(table_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(TableAssetCounts {
        seen: stored_count(&row, "assets_seen")?,
        copied: stored_count(&row, "assets_copied")?,
        reused: stored_count(&row, "assets_reused")?,
        failed: stored_count(&row, "assets_failed")?,
    })
}

pub(super) async fn persist_table_asset_counts(
    state: &AppState,
    run_id: Uuid,
    wiki_token: &str,
    table_id: &str,
    counts: TableAssetCounts,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"UPDATE feishu_run_table_results SET
             assets_seen=$4,assets_copied=$5,assets_reused=$6,assets_failed=$7
           WHERE sync_run_id=$1 AND wiki_token=$2 AND table_id=$3"#,
    )
    .bind(run_id)
    .bind(wiki_token)
    .bind(table_id)
    .bind(signed_count(counts.seen))
    .bind(signed_count(counts.copied))
    .bind(signed_count(counts.reused))
    .bind(signed_count(counts.failed))
    .execute(&state.pool)
    .await?;
    Ok(())
}

fn stored_count(row: &sqlx::postgres::PgRow, name: &str) -> Result<u64, ApiError> {
    u64::try_from(row.try_get::<i64, _>(name)?)
        .map_err(|_| ApiError::service_unavailable("Stored table asset count is invalid."))
}

fn signed_count(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

pub(super) fn runtime_table_mapping(
    mapping: &VersionedFeishuMapping,
    source: &DiscoveredSource,
) -> Result<FeishuTableMapping, ApiError> {
    let validated = super::validate_mapping(mapping.clone(), std::slice::from_ref(source))?;
    validated
        .tables
        .get(&super::source_identity_key(&source.source))
        .or_else(|| validated.tables.get(&source.source.table_id))
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

pub(super) async fn start_table(
    state: &AppState,
    run_id: Uuid,
    wiki_token: &str,
    table_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"UPDATE feishu_run_table_results SET status='fetching',error=NULL
           WHERE sync_run_id=$1 AND wiki_token=$2 AND table_id=$3"#,
    )
    .bind(run_id)
    .bind(wiki_token)
    .bind(table_id)
    .execute(&state.pool)
    .await?;
    Ok(())
}

pub(super) async fn fail_table(
    state: &AppState,
    run_id: Uuid,
    wiki_token: &str,
    table_id: &str,
    detail: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"UPDATE feishu_run_table_results result SET
             status='failed',error=$4,
             records_seen=(SELECT count(*) FROM staging_records staging
                           JOIN source_snapshots snapshot
                             ON snapshot.id=staging.source_snapshot_id
                           WHERE staging.sync_run_id=$1
                             AND snapshot.source_payload->>'wikiToken'=$2
                             AND snapshot.source_payload->>'tableId'=$3),
             records_applied=(SELECT count(*) FROM feishu_sync_changes changes
                              JOIN feishu_product_ownership ownership
                                ON ownership.product_id=changes.product_id
                              WHERE changes.sync_run_id=$1
                                AND ownership.wiki_token=$2
                                AND ownership.table_id=$3),
             records_failed=(SELECT count(*) FROM staging_records staging
                             JOIN source_snapshots snapshot
                               ON snapshot.id=staging.source_snapshot_id
                             WHERE staging.sync_run_id=$1
                               AND snapshot.source_payload->>'wikiToken'=$2
                               AND snapshot.source_payload->>'tableId'=$3
                               AND staging.validation_status='invalid'),
             completed_at=now()
           WHERE sync_run_id=$1 AND wiki_token=$2 AND table_id=$3"#,
    )
    .bind(run_id)
    .bind(wiki_token)
    .bind(table_id)
    .bind(detail)
    .execute(&state.pool)
    .await?;
    increment_table_failure(state, run_id).await
}

pub(super) async fn complete_table(
    state: &AppState,
    run_id: Uuid,
    wiki_token: &str,
    table_id: &str,
    deleted: u64,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"UPDATE feishu_run_table_results result SET
             status='completed',
             records_seen=(SELECT count(*) FROM staging_records staging
                           JOIN source_snapshots snapshot
                             ON snapshot.id=staging.source_snapshot_id
                           WHERE staging.sync_run_id=$1
                             AND snapshot.source_payload->>'wikiToken'=$2
                             AND snapshot.source_payload->>'tableId'=$3),
             records_applied=(SELECT count(*) FROM feishu_sync_changes changes
                              JOIN feishu_product_ownership ownership
                                ON ownership.product_id=changes.product_id
                              WHERE changes.sync_run_id=$1
                                AND ownership.wiki_token=$2
                                AND ownership.table_id=$3),
             records_failed=(SELECT count(*) FROM staging_records staging
                             JOIN source_snapshots snapshot
                               ON snapshot.id=staging.source_snapshot_id
                             WHERE staging.sync_run_id=$1
                               AND snapshot.source_payload->>'wikiToken'=$2
                               AND snapshot.source_payload->>'tableId'=$3
                               AND staging.validation_status='invalid'),
             records_deleted=$4,completed_at=now()
           WHERE result.sync_run_id=$1 AND result.wiki_token=$2
             AND result.table_id=$3"#,
    )
    .bind(run_id)
    .bind(wiki_token)
    .bind(table_id)
    .bind(i64::try_from(deleted).unwrap_or(i64::MAX))
    .execute(&state.pool)
    .await?;
    Ok(())
}

pub(super) async fn finalize_run(
    state: &AppState,
    run_id: Uuid,
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
    let deleted: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(sum(records_deleted),0)::bigint
           FROM feishu_run_table_results WHERE sync_run_id=$1"#,
    )
    .bind(run_id)
    .fetch_one(&state.pool)
    .await?;
    let status = if failed > 0 || assets_failed > 0 {
        "completedWithErrors"
    } else {
        "completed"
    };
    sqlx::query(
        r#"UPDATE sync_runs SET status=$2,resume_cursor=NULL,records_seen=$3,records_valid=$4,
                  records_applied=$5,records_failed=$6,records_deleted=$7,
                  assets_seen=$8,assets_copied=$9,assets_reused=$10,assets_failed=$11,completed_at=now(),
                  payload=jsonb_build_object('status',$2)
           WHERE id=$1"#,
    )
    .bind(run_id)
    .bind(status)
    .bind(counts.try_get::<i64, _>("seen")?)
    .bind(counts.try_get::<i64, _>("valid")?)
    .bind(applied)
    .bind(failed)
    .bind(deleted)
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
    Ok(())
}
