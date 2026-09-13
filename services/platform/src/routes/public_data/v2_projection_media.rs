use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, models::ResolvedMedia};

pub(super) async fn resolve_published_media(
    pool: &sqlx::PgPool,
    content_id: Uuid,
    revision: i64,
) -> Result<Vec<ResolvedMedia>, ApiError> {
    let current_and_complete: Option<(bool, bool)> = sqlx::query_as(
        r#"SELECT COALESCE(entry.cms_published_revision=$2,false),
                  cms_publication_dependency_snapshot_complete($1,$2)
           FROM content_entries entry WHERE entry.id=$1"#,
    )
    .bind(content_id)
    .bind(revision)
    .fetch_optional(pool)
    .await?;
    if current_and_complete != Some((true, true)) {
        return Err(resolution_failed(content_id, revision));
    }
    let rows = sqlx::query(
        r#"SELECT DISTINCT asset.id,asset.media_type,asset.byte_size,asset.original_name
           FROM cms_publication_dependencies dependency
           JOIN media_assets asset
             ON asset.id=dependency.target_media_asset_id
            AND asset.deleted_at IS NULL
           WHERE dependency.source_content_id=$1
             AND dependency.source_revision=$2
             AND dependency.dependency_kind IN ('mediaInline','mediaDownload')
           ORDER BY asset.id"#,
    )
    .bind(content_id)
    .bind(revision)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            let asset_id: Uuid = row.try_get("id")?;
            Ok(ResolvedMedia {
                asset_id,
                public_url: format!("/api/public/v1/media/{asset_id}"),
                download_url: format!("/api/public/v1/media/{asset_id}/download"),
                media_type: row.try_get("media_type")?,
                byte_size: row.try_get("byte_size")?,
                original_name: row.try_get("original_name")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(ApiError::from)
}

fn resolution_failed(content_id: Uuid, revision: i64) -> ApiError {
    tracing::error!(
        %content_id,
        content_revision = revision,
        "published CMS dependency snapshot is missing or incomplete"
    );
    ApiError::service_unavailable("Published media dependencies are temporarily unavailable.")
        .with_code(crate::error::CONTENT_DEPENDENCY_CONFLICT)
}
