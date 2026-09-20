use axum::http::StatusCode;
use sqlx::{postgres::PgConnection, Row};

use crate::{
    error::ApiError,
    models::{ContentDraftV2, ContentTypeFields},
};

pub(super) async fn validate_site_icon(
    connection: &mut PgConnection,
    draft: &ContentDraftV2,
) -> Result<(), ApiError> {
    let ContentTypeFields::GeneralInformation(fields) = &draft.type_fields else {
        return Ok(());
    };
    let Some(reference) = &fields.site_icon else {
        return Ok(());
    };
    let row = sqlx::query(
        r#"SELECT deleted_at,scan_status,access_level,media_type,
                  original_width,original_height
           FROM media_assets WHERE id=$1"#,
    )
    .bind(reference.asset_id)
    .fetch_optional(connection)
    .await?;
    let Some(row) = row else {
        return Err(blocked("The selected site icon does not exist."));
    };
    let deleted: Option<chrono::DateTime<chrono::Utc>> = row.try_get("deleted_at")?;
    let media_type: String = row.try_get("media_type")?;
    let width: Option<i32> = row.try_get("original_width")?;
    let height: Option<i32> = row.try_get("original_height")?;
    if !valid_site_icon_metadata(
        deleted.is_some(),
        &row.try_get::<String, _>("scan_status")?,
        &row.try_get::<String, _>("access_level")?,
        &media_type,
        width,
        height,
    ) {
        return Err(blocked(
            "The site icon must be a clean public PNG, JPEG or WebP image, square, and at least 512 by 512 pixels.",
        ));
    }
    Ok(())
}

fn valid_site_icon_metadata(
    deleted: bool,
    scan_status: &str,
    access_level: &str,
    media_type: &str,
    width: Option<i32>,
    height: Option<i32>,
) -> bool {
    !deleted
        && scan_status == "clean"
        && access_level == "public"
        && matches!(media_type, "image/png" | "image/jpeg" | "image/webp")
        && valid_site_icon_dimensions(width, height)
}

fn valid_site_icon_dimensions(width: Option<i32>, height: Option<i32>) -> bool {
    matches!((width, height), (Some(width), Some(height)) if width == height && width >= 512)
}

fn blocked(detail: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Publication blocked",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::{valid_site_icon_dimensions, valid_site_icon_metadata};

    #[test]
    fn site_icon_dimensions_are_square_and_at_least_512_pixels() {
        assert!(valid_site_icon_dimensions(Some(512), Some(512)));
        assert!(valid_site_icon_dimensions(Some(1024), Some(1024)));
        assert!(!valid_site_icon_dimensions(Some(511), Some(511)));
        assert!(!valid_site_icon_dimensions(Some(512), Some(513)));
        assert!(!valid_site_icon_dimensions(None, Some(512)));
    }

    #[test]
    fn site_icon_metadata_requires_clean_public_supported_image() {
        let valid = |deleted, scan_status, access_level, media_type| {
            valid_site_icon_metadata(
                deleted,
                scan_status,
                access_level,
                media_type,
                Some(512),
                Some(512),
            )
        };
        assert!(valid(false, "clean", "public", "image/png"));
        assert!(!valid(true, "clean", "public", "image/png"));
        assert!(!valid(false, "pending", "public", "image/png"));
        assert!(!valid(false, "clean", "private", "image/png"));
        assert!(!valid(false, "clean", "public", "image/svg+xml"));
    }
}
