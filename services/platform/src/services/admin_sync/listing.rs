use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        CursorPage, FeishuConnectionStatus, StagingRecord, StagingRecordPage,
        StagingValidationStatus, SyncMapping, SyncRun,
    },
    pagination::{
        cursor_limit, decode_scoped_cursor_compat, encode_scoped_cursor, CursorQuery, DecodedCursor,
    },
    services::request_metrics::LegacyCursorEndpoint,
    state::AppState,
};

#[derive(Deserialize, Serialize)]
struct SyncRunCursor {
    started_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Deserialize, Serialize)]
struct MappingCursor {
    active: bool,
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Deserialize, Serialize)]
struct TimeCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

pub struct StagingFilter {
    pub sync_run_id: Option<Uuid>,
    pub status: Option<String>,
    pub search: Option<String>,
}

pub async fn list_sync_runs(
    state: &AppState,
    query: CursorQuery,
) -> Result<CursorPage<SyncRun>, ApiError> {
    let scope = "admin.feishuSyncRuns";
    let limit = cursor_limit(&query)?;
    let after = match query.cursor.as_deref() {
        Some(value) => match decode_scoped_cursor_compat::<Uuid, SyncRunCursor>(scope, value)? {
            DecodedCursor::Current(cursor) => Some(cursor),
            DecodedCursor::Legacy(id) => {
                let started_at = sqlx::query_scalar("SELECT started_at FROM sync_runs WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&state.pool)
                    .await?
                    .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
                state
                    .request_metrics
                    .record_legacy_cursor(LegacyCursorEndpoint::SyncRuns);
                Some(SyncRunCursor { started_at, id })
            }
        },
        None => None,
    };
    let rows = sqlx::query(
        r#"SELECT id,connector_id,source,dry_run,run_kind,mapping_version,status,
                  resume_cursor,records_seen,records_valid,conflict_count,records_applied,
                  records_failed,assets_seen,assets_copied,assets_reused,assets_failed,
                  started_at,completed_at,payload FROM sync_runs
           WHERE ($1::timestamptz IS NULL OR (started_at,id)<($1,$2))
           ORDER BY started_at DESC,id DESC LIMIT $3"#,
    )
    .bind(after.as_ref().map(|value| value.started_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .iter()
        .map(crate::services::feishu::decode_sync_run)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = has_more
        .then(|| items.last())
        .flatten()
        .map(|item| {
            encode_scoped_cursor(
                scope,
                &SyncRunCursor {
                    started_at: item.started_at,
                    id: item.id,
                },
            )
        })
        .transpose()?;
    Ok(CursorPage { items, next_cursor })
}

pub async fn connection_status(state: &AppState) -> Result<FeishuConnectionStatus, ApiError> {
    let connector = sqlx::query(
        r#"SELECT connector.id,connector.display_name,connector.enabled,connector.updated_at,
                  EXISTS(SELECT 1 FROM object_storage_settings WHERE singleton=true) AS storage_ready,
                  EXISTS(SELECT 1 FROM sync_mappings mapping
                         WHERE mapping.connector_id=connector.id AND mapping.active) AS mapping_ready
           FROM source_connectors connector WHERE connector.connector_type='feishu'
           ORDER BY connector.updated_at DESC LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?;
    let latest_sync = latest_sync_run(state).await?;
    Ok(match connector {
        Some(row) => {
            let credentials = state.feishu_client.configured();
            let enabled: bool = row.try_get("enabled")?;
            let storage_ready: bool = row.try_get("storage_ready")?;
            let mapping_ready: bool = row.try_get("mapping_ready")?;
            let encryption_ready = state.config.product_staging_encryption_key.is_some();
            let unavailable_reason = if !credentials {
                Some("Feishu deployment credentials are not configured.".into())
            } else if !storage_ready {
                Some("Object storage is not configured.".into())
            } else if !encryption_ready {
                Some("Product staging encryption is not configured.".into())
            } else if !mapping_ready {
                Some("Run the connection test to discover and validate the field mapping.".into())
            } else if !enabled {
                Some("Automatic synchronization is disabled.".into())
            } else {
                None
            };
            FeishuConnectionStatus {
                connector_id: Some(row.try_get("id")?),
                display_name: Some(row.try_get("display_name")?),
                configured: credentials,
                enabled,
                runnable: credentials
                    && enabled
                    && storage_ready
                    && mapping_ready
                    && encryption_ready,
                unavailable_reason,
                updated_at: Some(row.try_get("updated_at")?),
                latest_sync,
            }
        }
        None => FeishuConnectionStatus {
            connector_id: None,
            display_name: None,
            configured: false,
            enabled: false,
            runnable: false,
            unavailable_reason: Some("Feishu connector settings are not initialized.".into()),
            updated_at: None,
            latest_sync,
        },
    })
}

async fn latest_sync_run(state: &AppState) -> Result<Option<SyncRun>, ApiError> {
    sqlx::query(
        r#"SELECT id,connector_id,source,dry_run,run_kind,mapping_version,status,
                  resume_cursor,records_seen,records_valid,conflict_count,records_applied,
                  records_failed,assets_seen,assets_copied,assets_reused,assets_failed,
                  started_at,completed_at,payload
           FROM sync_runs ORDER BY started_at DESC,id DESC LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?
    .as_ref()
    .map(crate::services::feishu::decode_sync_run)
    .transpose()
}

pub async fn list_mappings(
    state: &AppState,
    query: CursorQuery,
) -> Result<CursorPage<SyncMapping>, ApiError> {
    let scope = "admin.feishuMappings";
    let limit = cursor_limit(&query)?;
    let after = resolve_mapping_cursor(state, scope, query.cursor.as_deref()).await?;
    let rows = sqlx::query(
        r#"SELECT id,connector_id,version,mapping,schema_version,active,created_at
           FROM sync_mappings WHERE ($1::boolean IS NULL OR active<$1
             OR (active=$1 AND (created_at,id)<($2,$3)))
           ORDER BY active DESC,created_at DESC,id DESC LIMIT $4"#,
    )
    .bind(after.as_ref().map(|value| value.active))
    .bind(after.as_ref().map(|value| value.created_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .into_iter()
        .map(decode_mapping)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = has_more
        .then(|| items.last())
        .flatten()
        .map(|item| {
            encode_scoped_cursor(
                scope,
                &MappingCursor {
                    active: item.active,
                    created_at: item.created_at,
                    id: item.id,
                },
            )
        })
        .transpose()?;
    Ok(CursorPage { items, next_cursor })
}

async fn resolve_mapping_cursor(
    state: &AppState,
    scope: &str,
    value: Option<&str>,
) -> Result<Option<MappingCursor>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    match decode_scoped_cursor_compat::<Uuid, MappingCursor>(scope, value)? {
        DecodedCursor::Current(cursor) => Ok(Some(cursor)),
        DecodedCursor::Legacy(id) => {
            let row = sqlx::query("SELECT active,created_at FROM sync_mappings WHERE id=$1")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::FeishuMappings);
            Ok(Some(MappingCursor {
                active: row.try_get("active")?,
                created_at: row.try_get("created_at")?,
                id,
            }))
        }
    }
}

pub async fn list_staging(
    state: &AppState,
    filter: StagingFilter,
    query: CursorQuery,
) -> Result<StagingRecordPage, ApiError> {
    let scope = format!(
        "admin.feishuStaging|{:?}|{:?}|{:?}",
        filter.sync_run_id, filter.status, filter.search
    );
    let limit = cursor_limit(&query)?;
    let after = resolve_staging_cursor(state, &filter, &scope, query.cursor.as_deref()).await?;
    let total = staging_count(state, &filter).await?;
    let rows = sqlx::query(
        r#"SELECT id,sync_run_id,source_snapshot_id,source_record_id,validation_status,
                  normalized_payload,validation_errors,created_at FROM staging_records
           WHERE ($1::uuid IS NULL OR sync_run_id=$1)
             AND ($2::text IS NULL OR validation_status=$2)
             AND ($3::text IS NULL OR source_record_id ILIKE '%' || $3 || '%')
             AND ($4::timestamptz IS NULL OR (created_at,id)<($4,$5))
           ORDER BY created_at DESC,id DESC LIMIT $6"#,
    )
    .bind(filter.sync_run_id)
    .bind(filter.status.as_deref())
    .bind(filter.search.as_deref())
    .bind(after.as_ref().map(|value| value.created_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .into_iter()
        .map(decode_staging)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = has_more
        .then(|| items.last())
        .flatten()
        .map(|item| {
            encode_scoped_cursor(
                &scope,
                &TimeCursor {
                    created_at: item.created_at,
                    id: item.id,
                },
            )
        })
        .transpose()?;
    Ok(StagingRecordPage {
        items,
        next_cursor,
        total: usize::try_from(total).unwrap_or(usize::MAX),
    })
}

async fn resolve_staging_cursor(
    state: &AppState,
    filter: &StagingFilter,
    scope: &str,
    value: Option<&str>,
) -> Result<Option<TimeCursor>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    match decode_scoped_cursor_compat::<Uuid, TimeCursor>(scope, value)? {
        DecodedCursor::Current(cursor) => Ok(Some(cursor)),
        DecodedCursor::Legacy(id) => {
            let created_at = sqlx::query_scalar(
                r#"SELECT created_at FROM staging_records WHERE id=$1
                   AND ($2::uuid IS NULL OR sync_run_id=$2)
                   AND ($3::text IS NULL OR validation_status=$3)
                   AND ($4::text IS NULL OR source_record_id ILIKE '%' || $4 || '%')"#,
            )
            .bind(id)
            .bind(filter.sync_run_id)
            .bind(filter.status.as_deref())
            .bind(filter.search.as_deref())
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::FeishuStaging);
            Ok(Some(TimeCursor { created_at, id }))
        }
    }
}

async fn staging_count(state: &AppState, filter: &StagingFilter) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        r#"SELECT count(*) FROM staging_records
           WHERE ($1::uuid IS NULL OR sync_run_id=$1)
             AND ($2::text IS NULL OR validation_status=$2)
             AND ($3::text IS NULL OR source_record_id ILIKE '%' || $3 || '%')"#,
    )
    .bind(filter.sync_run_id)
    .bind(filter.status.as_deref())
    .bind(filter.search.as_deref())
    .fetch_one(&state.pool)
    .await?)
}

fn decode_mapping(row: sqlx::postgres::PgRow) -> Result<SyncMapping, ApiError> {
    Ok(SyncMapping {
        id: row.try_get("id")?,
        connector_id: row.try_get("connector_id")?,
        version: row.try_get("version")?,
        mapping: row.try_get("mapping")?,
        schema_version: row.try_get("schema_version")?,
        active: row.try_get("active")?,
        created_at: row.try_get("created_at")?,
    })
}

fn decode_staging(row: sqlx::postgres::PgRow) -> Result<StagingRecord, ApiError> {
    Ok(StagingRecord {
        id: row.try_get("id")?,
        sync_run_id: row.try_get("sync_run_id")?,
        source_snapshot_id: row.try_get("source_snapshot_id")?,
        source_record_id: row.try_get("source_record_id")?,
        validation_status: serde_json::from_value::<StagingValidationStatus>(Value::String(
            row.try_get("validation_status")?,
        ))
        .map_err(|_| ApiError::service_unavailable("Stored staging status is invalid."))?,
        normalized_payload: row.try_get("normalized_payload")?,
        validation_errors: serde_json::from_value(row.try_get("validation_errors")?)
            .map_err(|_| ApiError::service_unavailable("Stored validation errors are invalid."))?,
        created_at: row.try_get("created_at")?,
    })
}
