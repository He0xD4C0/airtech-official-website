use axum::{
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, services::object_storage_settings, state::AppState};

/// Compatibility endpoint for URLs emitted before direct external media URLs
/// were introduced. The immutable destination is stored with each asset.
pub async fn deliver_media_asset(
    state: &AppState,
    asset_id: Uuid,
    download: bool,
    _request_headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let row = sqlx::query(
        r#"SELECT public_url,preview_public_url,storage_key,media_type,byte_size,original_name
           FROM media_assets WHERE id=$1 AND deleted_at IS NULL
             AND scan_status='clean' AND access_level='public'"#,
    )
    .bind(asset_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Media asset was not found."))?;
    if download {
        return download_response(state, &row).await;
    }
    let original_url: Option<String> = row.try_get("public_url")?;
    let original_url = original_url.ok_or_else(|| {
        ApiError::service_unavailable(
            "This legacy media asset has not been adopted into object storage settings.",
        )
    })?;
    let preview_url: Option<String> = row.try_get("preview_public_url")?;
    let media_type: String = row.try_get("media_type")?;
    let public_url = match preview_url {
        Some(url) => url,
        None if is_raster_preview_type(&media_type) => original_url,
        None => {
            return Err(ApiError::not_found(
                "This media asset has no public preview.",
            ))
        }
    };
    if !(public_url.starts_with("http://") || public_url.starts_with("https://")) {
        tracing::error!(%asset_id, "stored media public URL is invalid");
        return Err(ApiError::service_unavailable(
            "The stored media location is invalid.",
        ));
    }
    let response = Redirect::permanent(&public_url).into_response();
    debug_assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    Ok(response)
}

fn is_raster_preview_type(value: &str) -> bool {
    matches!(value, "image/png" | "image/jpeg" | "image/webp")
}

async fn download_response(
    state: &AppState,
    row: &sqlx::postgres::PgRow,
) -> Result<Response, ApiError> {
    let storage_key: String = row.try_get("storage_key")?;
    let media_type: String = row.try_get("media_type")?;
    let byte_size: i64 = row.try_get("byte_size")?;
    let expected_size = u64::try_from(byte_size)
        .map_err(|_| ApiError::service_unavailable("Stored media size is invalid."))?;
    let settings = object_storage_settings::active_storage(state).await?;
    let object = super::get_object(&settings, &storage_key, expected_size).await?;
    let content_type = HeaderValue::from_str(&media_type)
        .map_err(|_| ApiError::service_unavailable("Stored media type is invalid."))?;
    let original_name: String = row.try_get("original_name")?;
    let disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        safe_download_name(&original_name)
    ))
    .map_err(|_| ApiError::service_unavailable("Stored media filename is invalid."))?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, object.content_length)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        )
        .body(object.body)
        .map_err(|_| ApiError::internal("Media download response could not be built."))
}

fn safe_download_name(value: &str) -> String {
    let name = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' | ' ' => character,
            _ => '_',
        })
        .take(180)
        .collect::<String>();
    if name.trim_matches([' ', '.']).is_empty() {
        "download".into()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::{is_raster_preview_type, safe_download_name};

    #[test]
    fn download_names_cannot_inject_headers_or_paths() {
        let name = safe_download_name("../报价\r\n.xls");
        assert!(name.ends_with(".xls"));
        assert!(!name.contains(['/', '\\', '\r', '\n', '"']));
        assert_eq!(safe_download_name(""), "download");
    }

    #[test]
    fn cad_mime_types_are_not_treated_as_raster_previews() {
        assert!(is_raster_preview_type("image/webp"));
        assert!(!is_raster_preview_type("image/vnd.dwg"));
        assert!(!is_raster_preview_type("image/vnd.dxf"));
    }
}
