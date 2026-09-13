use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{CursorPage, MediaAsset, MediaAssetReference},
    pagination::{cursor_limit, decode_scoped_cursor, encode_scoped_cursor, CursorQuery},
    services::cms_content::require_postgres,
    state::AppState,
};

#[derive(Clone, Debug, Default)]
pub struct MediaAssetFilter {
    pub query: Option<String>,
}

impl MediaAssetFilter {
    pub fn parse(query: Option<String>) -> Result<Self, ApiError> {
        let query = query
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if query
            .as_ref()
            .is_some_and(|value| value.chars().count() > 200)
        {
            return Err(ApiError::bad_request("q must be at most 200 characters."));
        }
        Ok(Self { query })
    }

    fn cursor_scope(&self) -> String {
        format!(
            "admin.media.assets|q={}",
            self.query.as_deref().unwrap_or("")
        )
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAssetPage {
    pub items: Vec<MediaAsset>,
    pub next_cursor: Option<String>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaAssetCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

pub async fn list_media_assets(
    state: &AppState,
    filter: MediaAssetFilter,
    pagination: CursorQuery,
) -> Result<MediaAssetPage, ApiError> {
    let pool = require_postgres(state)?;
    let limit = cursor_limit(&pagination)?;
    let scope = filter.cursor_scope();
    let cursor = pagination
        .cursor
        .as_deref()
        .map(|value| decode_scoped_cursor::<MediaAssetCursor>(&scope, value))
        .transpose()?;
    let pattern = filter
        .query
        .as_deref()
        .map(|value| format!("%{}%", escape_like(value)));
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM media_assets
           WHERE deleted_at IS NULL
             AND ($1::text IS NULL OR original_name ILIKE $1 ESCAPE '\')"#,
    )
    .bind(&pattern)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query(
        r#"SELECT id,original_name,media_type,byte_size,checksum,
                  COALESCE(uploaded_by,'legacy') AS uploaded_by,created_at
           FROM media_assets
           WHERE deleted_at IS NULL
             AND ($1::text IS NULL OR original_name ILIKE $1 ESCAPE '\')
             AND ($2::timestamptz IS NULL OR (created_at,id) < ($2,$3))
           ORDER BY created_at DESC,id DESC
           LIMIT $4"#,
    )
    .bind(pattern)
    .bind(cursor.as_ref().map(|value| value.created_at))
    .bind(cursor.as_ref().map(|value| value.id))
    .bind((limit + 1) as i64)
    .fetch_all(pool)
    .await?;
    let mut items = rows
        .iter()
        .map(decode_media_asset)
        .collect::<Result<Vec<_>, ApiError>>()?;
    let has_more = items.len() > limit;
    if has_more {
        items.truncate(limit);
    }
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| {
                encode_scoped_cursor(
                    &scope,
                    &MediaAssetCursor {
                        created_at: item.created_at,
                        id: item.id,
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(MediaAssetPage {
        items,
        next_cursor,
        total,
    })
}

pub async fn load_media_asset(pool: &sqlx::PgPool, asset_id: Uuid) -> Result<MediaAsset, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,original_name,media_type,byte_size,checksum,
                  COALESCE(uploaded_by,'legacy') AS uploaded_by,created_at
           FROM media_assets WHERE id=$1 AND deleted_at IS NULL"#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Media asset was not found."))?;
    decode_media_asset(&row)
}

pub async fn list_media_references(
    state: &AppState,
    asset_id: Uuid,
    pagination: CursorQuery,
) -> Result<CursorPage<MediaAssetReference>, ApiError> {
    let pool = require_postgres(state)?;
    load_media_asset(pool, asset_id).await?;
    let limit = cursor_limit(&pagination)?;
    let offset = pagination
        .cursor
        .as_deref()
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| ApiError::bad_request("The media reference cursor is invalid."))?
        .unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::bad_request(
            "The media reference cursor is invalid.",
        ));
    }
    let rows = sqlx::query(
        r#"SELECT dependency.source_content_id,dependency.source_revision,
                  entry.title,entry.status,dependency.dependency_kind,
                  dependency.reference_path
           FROM cms_publication_dependencies dependency
           JOIN content_entries entry ON entry.id=dependency.source_content_id
           WHERE dependency.target_media_asset_id=$1
           ORDER BY dependency.created_at DESC,dependency.id DESC
           OFFSET $2 LIMIT $3"#,
    )
    .bind(asset_id)
    .bind(offset)
    .bind((limit + 1) as i64)
    .fetch_all(pool)
    .await?;
    let has_more = rows.len() > limit;
    let items = rows
        .iter()
        .take(limit)
        .map(|row| {
            Ok(MediaAssetReference {
                content_id: row.try_get("source_content_id")?,
                content_revision: row.try_get("source_revision")?,
                content_title: row.try_get("title")?,
                content_status: row.try_get("status")?,
                dependency_kind: row.try_get("dependency_kind")?,
                reference_path: row.try_get("reference_path")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let next_cursor = has_more.then(|| (offset + limit as i64).to_string());
    Ok(CursorPage { items, next_cursor })
}

pub(crate) fn decode_media_asset(row: &PgRow) -> Result<MediaAsset, ApiError> {
    let id: Uuid = row.try_get("id")?;
    Ok(MediaAsset {
        id,
        public_url: format!("/api/public/v1/media/{id}"),
        download_url: format!("/api/public/v1/media/{id}/download"),
        original_name: row.try_get("original_name")?,
        media_type: row.try_get("media_type")?,
        byte_size: row.try_get("byte_size")?,
        sha256: row.try_get("checksum")?,
        uploaded_by: row.try_get("uploaded_by")?,
        created_at: row.try_get("created_at")?,
    })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
