use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{CursorPage, MediaAssetSummary},
    pagination::{cursor_limit, decode_scoped_cursor, encode_scoped_cursor, CursorQuery},
    services::cms_content::require_postgres,
    services::cms_preflight::stable_media_version_id,
    state::AppState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaScanStatus {
    Pending,
    Clean,
    Quarantined,
    Failed,
}

impl MediaScanStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Clean => "clean",
            Self::Quarantined => "quarantined",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "clean" => Some(Self::Clean),
            "quarantined" => Some(Self::Quarantined),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaAccessLevel {
    Public,
    Authenticated,
    Internal,
}

impl MediaAccessLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Authenticated => "authenticated",
            Self::Internal => "internal",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "authenticated" => Some(Self::Authenticated),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MediaAssetFilter {
    pub query: Option<String>,
    pub scan_status: Option<MediaScanStatus>,
    pub access_level: Option<MediaAccessLevel>,
}

impl MediaAssetFilter {
    pub fn parse(
        query: Option<String>,
        scan_status: Option<String>,
        access_level: Option<String>,
    ) -> Result<Self, ApiError> {
        let query = query
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if query
            .as_ref()
            .is_some_and(|value| value.chars().count() > 200)
        {
            return Err(ApiError::bad_request("q must be at most 200 characters."));
        }
        let scan_status = match scan_status.as_deref() {
            None => None,
            Some(value) => Some(MediaScanStatus::parse(value).ok_or_else(|| {
                ApiError::bad_request(
                    "scanStatus must be one of pending, clean, quarantined, or failed.",
                )
            })?),
        };
        let access_level = match access_level.as_deref() {
            None => None,
            Some(value) => Some(MediaAccessLevel::parse(value).ok_or_else(|| {
                ApiError::bad_request(
                    "accessLevel must be one of public, authenticated, or internal.",
                )
            })?),
        };
        Ok(Self {
            query,
            scan_status,
            access_level,
        })
    }

    fn cursor_scope(&self) -> String {
        format!(
            "admin.media.assets|q={}|scan={}|access={}",
            self.query.as_deref().unwrap_or(""),
            self.scan_status.map(MediaScanStatus::label).unwrap_or(""),
            self.access_level.map(MediaAccessLevel::label).unwrap_or(""),
        )
    }
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
) -> Result<CursorPage<MediaAssetSummary>, ApiError> {
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
    let scan_label = filter.scan_status.map(MediaScanStatus::label);
    let access_label = filter.access_level.map(MediaAccessLevel::label);
    let rows = sqlx::query(
        r#"SELECT id, original_name, media_type, byte_size, scan_status, access_level, created_at
           FROM media_assets
           WHERE deleted_at IS NULL
             AND ($1::text IS NULL OR original_name ILIKE $1 ESCAPE '\')
             AND ($2::text IS NULL OR scan_status = $2)
             AND ($3::text IS NULL OR access_level = $3)
             AND ($4::timestamptz IS NULL OR (created_at, id) < ($4, $5))
           ORDER BY created_at DESC, id DESC
           LIMIT $6"#,
    )
    .bind(pattern)
    .bind(scan_label)
    .bind(access_label)
    .bind(cursor.as_ref().map(|value| value.created_at))
    .bind(cursor.as_ref().map(|value| value.id))
    .bind((limit + 1) as i64)
    .fetch_all(pool)
    .await?;

    let mut items: Vec<MediaAssetSummary> = rows
        .into_iter()
        .map(|row| {
            Ok(MediaAssetSummary {
                id: row.try_get("id")?,
                version_id: stable_media_version_id(row.try_get("id")?),
                original_name: row.try_get("original_name")?,
                media_type: row.try_get("media_type")?,
                byte_size: row.try_get("byte_size")?,
                scan_status: row.try_get("scan_status")?,
                access_level: row.try_get("access_level")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()?;

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
    Ok(CursorPage { items, next_cursor })
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
