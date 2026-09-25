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
                let started_at = sqlx::query_scalar(
                    "SELECT started_at FROM sync_runs WHERE id=$1 AND source='feishu' AND connector_id=$2",
                )
                    .bind(id)
                    .bind(crate::services::feishu::CONNECTOR_ID)
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
    let rows = sqlx::query(&format!(
        "{} WHERE source='feishu' AND connector_id=$4
         AND ($1::timestamptz IS NULL OR (started_at,id)<($1,$2))
         ORDER BY started_at DESC,id DESC LIMIT $3",
        crate::services::feishu::sync_run_select()
    ))
    .bind(after.as_ref().map(|value| value.started_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .bind(crate::services::feishu::CONNECTOR_ID)
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
                  settings.app_id IS NOT NULL AND settings.app_secret IS NOT NULL
                    AS credentials_ready,
                  COALESCE(
                    settings.connection_revision=settings.tested_connection_revision,
                    false
                  ) AS connection_test_ready,
                  EXISTS(SELECT 1 FROM jsonb_array_elements(settings.sources) source
                         WHERE COALESCE((source->>'enabled')::boolean,true)) AS source_ready,
                  EXISTS(SELECT 1 FROM sync_mappings mapping
                         WHERE mapping.connector_id=connector.id AND mapping.active
                           AND mapping.version=settings.mapping_version) AS mapping_ready
           FROM source_connectors connector
           JOIN feishu_connector_settings settings ON settings.connector_id=connector.id
           WHERE connector.connector_type='feishu'
           ORDER BY connector.updated_at DESC LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await?;
    let latest_sync = latest_sync_run(state).await?;
    Ok(match connector {
        Some(row) => {
            let credentials: bool = row.try_get("credentials_ready")?;
            let enabled: bool = row.try_get("enabled")?;
            let storage_ready: bool = row.try_get("storage_ready")?;
            let mapping_ready: bool = row.try_get("mapping_ready")?;
            let connection_test_ready: bool = row.try_get("connection_test_ready")?;
            let source_ready: bool = row.try_get("source_ready")?;
            let encryption_ready = state.config.product_staging_encryption_key.is_some();
            let unavailable_reason = if !credentials {
                Some("Configure Feishu application credentials in Admin.".into())
            } else if !source_ready {
                Some("Enable at least one Feishu product table in Admin.".into())
            } else if !storage_ready {
                Some("Object storage is not configured.".into())
            } else if !encryption_ready {
                Some("Product staging encryption is not configured.".into())
            } else if !connection_test_ready || !mapping_ready {
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
                    && connection_test_ready
                    && source_ready
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
    sqlx::query(&format!(
        "{} WHERE source='feishu' AND connector_id=$1
         ORDER BY started_at DESC,id DESC LIMIT 1",
        crate::services::feishu::sync_run_select()
    ))
    .bind(crate::services::feishu::CONNECTOR_ID)
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
           FROM sync_mappings WHERE connector_id=$5 AND ($1::boolean IS NULL OR active<$1
             OR (active=$1 AND (created_at,id)<($2,$3)))
           ORDER BY active DESC,created_at DESC,id DESC LIMIT $4"#,
    )
    .bind(after.as_ref().map(|value| value.active))
    .bind(after.as_ref().map(|value| value.created_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .bind(crate::services::feishu::CONNECTOR_ID)
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
            let row = sqlx::query(
                "SELECT active,created_at FROM sync_mappings WHERE id=$1 AND connector_id=$2",
            )
            .bind(id)
            .bind(crate::services::feishu::CONNECTOR_ID)
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
        r#"SELECT staging.id,staging.sync_run_id,staging.source_snapshot_id,
                  staging.source_record_id,staging.validation_status,
                  staging.normalized_payload,staging.validation_errors,staging.created_at
           FROM staging_records staging JOIN sync_runs run ON run.id=staging.sync_run_id
           WHERE run.source='feishu' AND run.connector_id=$7
             AND ($1::uuid IS NULL OR staging.sync_run_id=$1)
             AND ($2::text IS NULL OR staging.validation_status=$2)
             AND ($3::text IS NULL OR staging.source_record_id ILIKE '%' || $3 || '%')
             AND ($4::timestamptz IS NULL
                  OR (staging.created_at,staging.id)<($4,$5))
           ORDER BY staging.created_at DESC,staging.id DESC LIMIT $6"#,
    )
    .bind(filter.sync_run_id)
    .bind(filter.status.as_deref())
    .bind(filter.search.as_deref())
    .bind(after.as_ref().map(|value| value.created_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .bind(crate::services::feishu::CONNECTOR_ID)
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
                r#"SELECT staging.created_at FROM staging_records staging
                   JOIN sync_runs run ON run.id=staging.sync_run_id
                   WHERE staging.id=$1 AND run.source='feishu' AND run.connector_id=$5
                   AND ($2::uuid IS NULL OR staging.sync_run_id=$2)
                   AND ($3::text IS NULL OR staging.validation_status=$3)
                   AND ($4::text IS NULL
                        OR staging.source_record_id ILIKE '%' || $4 || '%')"#,
            )
            .bind(id)
            .bind(filter.sync_run_id)
            .bind(filter.status.as_deref())
            .bind(filter.search.as_deref())
            .bind(crate::services::feishu::CONNECTOR_ID)
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
        r#"SELECT count(*) FROM staging_records staging
           JOIN sync_runs run ON run.id=staging.sync_run_id
           WHERE run.source='feishu' AND run.connector_id=$4
             AND ($1::uuid IS NULL OR staging.sync_run_id=$1)
             AND ($2::text IS NULL OR staging.validation_status=$2)
             AND ($3::text IS NULL
                  OR staging.source_record_id ILIKE '%' || $3 || '%')"#,
    )
    .bind(filter.sync_run_id)
    .bind(filter.status.as_deref())
    .bind(filter.search.as_deref())
    .bind(crate::services::feishu::CONNECTOR_ID)
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
