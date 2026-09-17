pub(super) use sqlx::Row;
pub(super) use uuid::Uuid;

pub(super) use crate::{error::ApiError, models::ResolvedMedia};

pub(crate) async fn resolve_published_media(
    pool: &sqlx::PgPool,
    content_id: Uuid,
    revision: i64,
) -> Result<Vec<ResolvedMedia>, ApiError> {
    let current: Option<bool> = sqlx::query_scalar(
        r#"SELECT publication_version=$2
           FROM cms_published_content WHERE content_id=$1"#,
    )
    .bind(content_id)
    .bind(revision)
    .fetch_optional(pool)
    .await?;
    if current != Some(true) {
        return Err(resolution_failed(content_id, revision));
    }
    let rows = sqlx::query(
        r#"SELECT DISTINCT asset.id,asset.public_url,asset.media_type,asset.byte_size,asset.original_name
           FROM cms_current_publication_dependencies dependency
           JOIN media_assets asset
             ON asset.id=dependency.target_media_asset_id
            AND asset.deleted_at IS NULL
           WHERE dependency.source_content_id=$1
             AND dependency.dependency_kind IN ('mediaInline','mediaDownload')
           ORDER BY asset.id"#,
    )
    .bind(content_id)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            let asset_id: Uuid = row.try_get("id")?;
            let public_url: Option<String> = row.try_get("public_url")?;
            Ok(ResolvedMedia {
                asset_id,
                public_url: public_url.ok_or_else(|| {
                    ApiError::service_unavailable(
                        "Published media includes a legacy URL that has not been adopted.",
                    )
                })?,
                download_url: format!("/api/public/v1/media/{asset_id}/download"),
                media_type: row.try_get("media_type")?,
                byte_size: row.try_get("byte_size")?,
                original_name: row.try_get("original_name")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()
}

pub(crate) fn resolution_failed(content_id: Uuid, revision: i64) -> ApiError {
    tracing::error!(
        %content_id,
        content_revision = revision,
        "current published CMS dependencies are missing or incomplete"
    );
    ApiError::service_unavailable("Published media dependencies are temporarily unavailable.")
        .with_code(crate::error::CONTENT_DEPENDENCY_CONFLICT)
}
