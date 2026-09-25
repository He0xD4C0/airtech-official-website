use chrono::{DateTime, Utc};
use sqlx::{postgres::PgConnection, Row};
use uuid::Uuid;

use crate::models::{
    AssetVersionReference, ContentBlock, ContentDraftV2, ContentTypeFields, MediaUseReference,
};

use super::{publication_blocked, Result};

pub(super) async fn validate_published_media(
    connection: &mut PgConnection,
    draft: &ContentDraftV2,
) -> Result<()> {
    for (label, reference) in collect_media_references(draft) {
        let row = sqlx::query(
            r#"SELECT deleted_at,media_type,original_width,original_height
               FROM media_assets WHERE id=$1"#,
        )
        .bind(reference.asset_id)
        .fetch_optional(&mut *connection)
        .await?;
        let Some(row) = row else {
            return Err(publication_blocked(format!(
                "{label} references media asset {}, which does not exist.",
                reference.asset_id
            )));
        };
        let deleted: Option<DateTime<Utc>> = row.try_get("deleted_at")?;
        if deleted.is_some() {
            return Err(publication_blocked(format!(
                "{label} references media asset {} that was deleted.",
                reference.asset_id
            )));
        }
        if label == "The site icon" {
            validate_site_icon(&row, reference.asset_id)?;
        }
    }
    Ok(())
}

fn validate_site_icon(row: &sqlx::postgres::PgRow, asset_id: Uuid) -> Result<()> {
    let media_type: String = row.try_get("media_type")?;
    let width: Option<i32> = row.try_get("original_width")?;
    let height: Option<i32> = row.try_get("original_height")?;
    if !valid_site_icon_properties(&media_type, width, height) {
        return Err(publication_blocked(format!(
            "Site icon media asset {asset_id} must be a square PNG, JPEG, or WebP image at least 512 x 512 pixels."
        )));
    }
    Ok(())
}

fn valid_site_icon_properties(media_type: &str, width: Option<i32>, height: Option<i32>) -> bool {
    matches!(media_type, "image/png" | "image/jpeg" | "image/webp")
        && width == height
        && width.is_some_and(|value| value >= 512)
}

fn collect_media_references(draft: &ContentDraftV2) -> Vec<(String, AssetVersionReference)> {
    let mut references = Vec::new();
    if let Some(media) = &draft.seo.social_image {
        push_media(&mut references, "The SEO social image".to_owned(), media);
    }
    match &draft.type_fields {
        ContentTypeFields::GeneralInformation(fields) => {
            if let Some(site_icon) = &fields.site_icon {
                references.push(("The site icon".to_owned(), site_icon.clone()));
            }
        }
        ContentTypeFields::News(fields) | ContentTypeFields::Article(fields) => {
            if let Some(cover) = &fields.cover {
                push_media(&mut references, "The cover image".to_owned(), cover);
            }
        }
        _ => {}
    }
    for block in &draft.composition.blocks {
        match block {
            ContentBlock::Hero(hero) => {
                if let Some(media) = &hero.media {
                    push_media(&mut references, format!("Hero block {}", hero.id), media);
                }
            }
            ContentBlock::Media(media) => push_media(
                &mut references,
                format!("Media block {}", media.id),
                &media.media,
            ),
            ContentBlock::FeatureGrid(grid) => {
                for item in &grid.items {
                    if let Some(icon) = &item.icon {
                        push_media(&mut references, format!("Feature item {}", item.id), icon);
                    }
                }
            }
            ContentBlock::DownloadAsset(block) => {
                references.push((format!("Download block {}", block.id), block.asset.clone()));
            }
            _ => {}
        }
    }
    references
}

fn push_media(
    references: &mut Vec<(String, AssetVersionReference)>,
    label: String,
    media: &MediaUseReference,
) {
    references.push((label, media.asset.clone()));
}

#[cfg(test)]
mod tests {
    use super::valid_site_icon_properties;

    #[test]
    fn site_icon_requires_a_supported_square_image_at_least_512_pixels() {
        assert!(valid_site_icon_properties(
            "image/png",
            Some(512),
            Some(512)
        ));
        assert!(valid_site_icon_properties(
            "image/webp",
            Some(1024),
            Some(1024)
        ));
        assert!(!valid_site_icon_properties(
            "image/svg+xml",
            Some(512),
            Some(512)
        ));
        assert!(!valid_site_icon_properties(
            "image/jpeg",
            Some(512),
            Some(256)
        ));
        assert!(!valid_site_icon_properties(
            "image/png",
            Some(256),
            Some(256)
        ));
        assert!(!valid_site_icon_properties("image/png", None, None));
    }
}
