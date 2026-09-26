use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiError;
use crate::pagination::{
    cursor_limit, decode_scoped_cursor_compat, encode_scoped_cursor, CursorQuery, DecodedCursor,
};
use crate::services::{
    product_import::load_product_import_result, request_metrics::LegacyCursorEndpoint,
};
use crate::state::AppState;
use airtek_domain::models::{CursorPage, ProductImportResult, TemporaryOverride};

#[derive(Debug, Deserialize, Serialize)]
struct TimeCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

pub async fn list_product_imports(
    state: &AppState,
    query: CursorQuery,
) -> Result<CursorPage<ProductImportResult>, ApiError> {
    let scope = "admin.productImports";
    let limit = cursor_limit(&query)?;
    let after = resolve_import_cursor(state, scope, query.cursor.as_deref()).await?;
    let rows = sqlx::query(
        r#"SELECT id,created_at FROM product_import_runs
           WHERE ($1::timestamptz IS NULL OR (created_at,id)<($1,$2))
           ORDER BY created_at DESC,id DESC LIMIT $3"#,
    )
    .bind(after.as_ref().map(|value| value.created_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut results = Vec::with_capacity(rows.len());
    let mut positions = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.try_get("id")?;
        if let Some(result) = load_product_import_result(&state.pool, id).await? {
            results.push(result);
            positions.push(TimeCursor {
                created_at: row.try_get("created_at")?,
                id,
            });
        }
    }
    let has_more = results.len() > limit;
    results.truncate(limit);
    positions.truncate(limit);
    let next_cursor = has_more
        .then(|| positions.last())
        .flatten()
        .map(|position| encode_scoped_cursor(scope, position))
        .transpose()?;
    Ok(CursorPage {
        items: results,
        next_cursor,
    })
}

async fn resolve_import_cursor(
    state: &AppState,
    scope: &str,
    value: Option<&str>,
) -> Result<Option<TimeCursor>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    match decode_scoped_cursor_compat::<Uuid, TimeCursor>(scope, value)? {
        DecodedCursor::Current(cursor) => Ok(Some(cursor)),
        DecodedCursor::Legacy(id) => {
            let created_at =
                sqlx::query_scalar("SELECT created_at FROM product_import_runs WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&state.pool)
                    .await?
                    .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::AdminProductImports);
            Ok(Some(TimeCursor { created_at, id }))
        }
    }
}

pub async fn list_temporary_overrides(
    state: &AppState,
    product_id: Uuid,
    query: CursorQuery,
) -> Result<CursorPage<TemporaryOverride>, ApiError> {
    let scope = format!("admin.productOverrides|{product_id}");
    let limit = cursor_limit(&query)?;
    let after = resolve_override_cursor(state, product_id, &scope, query.cursor.as_deref()).await?;
    let rows = sqlx::query(
        r#"SELECT id,product_id,field_path,value,reason,created_at,expires_at
           FROM product_temporary_overrides WHERE product_id=$1
             AND ($2::timestamptz IS NULL OR (created_at,id)<($2,$3))
           ORDER BY created_at DESC,id DESC LIMIT $4"#,
    )
    .bind(product_id)
    .bind(after.as_ref().map(|value| value.created_at))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .into_iter()
        .map(decode_override)
        .collect::<Result<Vec<_>, ApiError>>()?;
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
    Ok(CursorPage { items, next_cursor })
}

async fn resolve_override_cursor(
    state: &AppState,
    product_id: Uuid,
    scope: &str,
    value: Option<&str>,
) -> Result<Option<TimeCursor>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    match decode_scoped_cursor_compat::<Uuid, TimeCursor>(scope, value)? {
        DecodedCursor::Current(cursor) => Ok(Some(cursor)),
        DecodedCursor::Legacy(id) => {
            let created_at = sqlx::query_scalar(
                "SELECT created_at FROM product_temporary_overrides WHERE id=$1 AND product_id=$2",
            )
            .bind(id)
            .bind(product_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::ProductOverrides);
            Ok(Some(TimeCursor { created_at, id }))
        }
    }
}

fn decode_override(row: sqlx::postgres::PgRow) -> Result<TemporaryOverride, ApiError> {
    let expires_at = row.try_get("expires_at")?;
    Ok(TemporaryOverride {
        id: row.try_get("id")?,
        product_id: row.try_get("product_id")?,
        field_path: row.try_get("field_path")?,
        value: row.try_get("value")?,
        reason: row.try_get("reason")?,
        created_at: row.try_get("created_at")?,
        expires_at,
        expired: expires_at <= Utc::now(),
    })
}
