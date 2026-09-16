use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

use super::storage;

/// Serve every live media asset without authentication. Content publication
/// controls whether a URL is referenced by the website, never whether a known
/// asset URL can be fetched.
pub async fn deliver_media_asset(
    state: &AppState,
    asset_id: Uuid,
    download: bool,
    request_headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let pool = &state.pool;
    let row = sqlx::query(
        r#"SELECT storage_key,media_type,original_name,checksum,storage_backend,byte_size
           FROM media_assets WHERE id=$1 AND deleted_at IS NULL"#,
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Media asset was not found."))?;
    let settings = state.config.media.storage.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Object storage is not configured for media delivery.")
    })?;
    let storage_backend: Option<String> = row.try_get("storage_backend")?;
    if storage_backend
        .as_deref()
        .is_some_and(|backend| backend != settings.kind.label())
    {
        tracing::error!(%asset_id, "media catalogue points at a different storage backend");
        return Err(ApiError::not_found("Media asset was not found."));
    }
    let checksum: String = row.try_get("checksum")?;
    let etag = strong_etag(&checksum);
    if request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| matches_etag(value, &etag))
    {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        apply_headers(
            response.headers_mut(),
            &etag,
            download,
            row.try_get("media_type")?,
            row.try_get("original_name")?,
        )?;
        return Ok(response);
    }
    let byte_size: i64 = row.try_get("byte_size")?;
    let expected_size = u64::try_from(byte_size)
        .map_err(|_| ApiError::service_unavailable("Stored media size is invalid."))?;
    let object = storage::get_object(settings, row.try_get("storage_key")?, expected_size).await?;
    let mut response = object.body.into_response();
    apply_headers(
        response.headers_mut(),
        &etag,
        download,
        row.try_get("media_type")?,
        row.try_get("original_name")?,
    )?;
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&object.content_length.to_string())
            .map_err(|_| ApiError::internal("Media content length is invalid."))?,
    );
    Ok(response)
}

fn strong_etag(checksum: &str) -> String {
    let normalized = checksum.strip_prefix("sha256:").unwrap_or(checksum);
    if normalized.len() == 64 && normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        format!("\"{}\"", normalized.to_ascii_lowercase())
    } else {
        format!(
            "\"{}\"",
            data_encoding::HEXLOWER.encode(&Sha256::digest(checksum.as_bytes()))
        )
    }
}

fn matches_etag(header_value: &str, etag: &str) -> bool {
    header_value
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate.trim_start_matches("W/") == etag)
}

fn apply_headers(
    headers: &mut HeaderMap,
    etag: &str,
    download: bool,
    media_type: String,
    original_name: String,
) -> Result<(), ApiError> {
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(etag).map_err(|_| ApiError::internal("Media ETag is invalid."))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&media_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition(download, &original_name))
            .unwrap_or_else(|_| HeaderValue::from_static("inline")),
    );
    Ok(())
}

fn content_disposition(download: bool, file_name: &str) -> String {
    let fallback = file_name
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() && character != '"' && character != '\\' {
                character
            } else {
                '-'
            }
        })
        .take(120)
        .collect::<String>();
    let fallback = if fallback.trim_matches('-').is_empty() {
        "media".to_owned()
    } else {
        fallback
    };
    let kind = if download { "attachment" } else { "inline" };
    format!("{kind}; filename=\"{fallback}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_used_as_the_strong_etag() {
        let checksum = "a".repeat(64);
        assert_eq!(strong_etag(&checksum), format!("\"{checksum}\""));
    }
}
