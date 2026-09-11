use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

use super::storage;

/// Delivers one immutable public object.
///
/// The gate is deliberately narrow: only a reviewed-clean, public, live asset
/// is readable, and every rejection answers `404` so the endpoint cannot be
/// used to enumerate quarantined or internal media.
pub async fn deliver_media_asset(
    state: &AppState,
    asset_id: Uuid,
    download: bool,
    request_headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("PostgreSQL persistence is required for media delivery.")
    })?;
    let row = sqlx::query(
        "SELECT storage_key,media_type,original_name,checksum,storage_backend,byte_size \
         FROM media_assets \
         WHERE id=$1 AND deleted_at IS NULL AND scan_status='clean' AND access_level='public'",
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Media asset was not found."))?;
    let storage_key: String = row.try_get("storage_key")?;
    let media_type: String = row.try_get("media_type")?;
    let original_name: String = row.try_get("original_name")?;
    let checksum: String = row.try_get("checksum")?;
    let storage_backend: Option<String> = row.try_get("storage_backend")?;
    let byte_size: i64 = row.try_get("byte_size")?;
    let expected_size = u64::try_from(byte_size).map_err(|_| {
        tracing::error!(asset_id = %asset_id, byte_size, "media catalogue length is invalid");
        ApiError::service_unavailable("Media catalogue metadata is invalid.")
    })?;
    let settings = state.config.media.storage.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Object storage is not configured for media delivery.")
    })?;
    if storage_backend
        .as_deref()
        .is_some_and(|backend| backend != settings.kind.label())
    {
        return Err(ApiError::service_unavailable(
            "The stored media object is owned by a different storage backend.",
        ));
    }
    let etag = strong_etag(&checksum);
    if request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| matches_etag(value, &etag))
    {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        apply_cache_headers(response.headers_mut(), &etag)?;
        return Ok(response);
    }
    let object = storage::get_object(settings, &storage_key, expected_size).await?;
    let mut response = object.body.into_response();
    apply_cache_headers(response.headers_mut(), &etag)?;
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&object.content_length.to_string())
            .map_err(|_| ApiError::internal("Media content length is invalid."))?,
    );
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&media_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition(download, &original_name))
            .unwrap_or_else(|_| HeaderValue::from_static("inline")),
    );
    Ok(response)
}

fn apply_cache_headers(headers: &mut HeaderMap, etag: &str) -> Result<(), ApiError> {
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
    Ok(())
}

/// A stable strong validator derived from the stored checksum. Deriving it
/// again keeps the header valid even for catalogue rows written before this
/// pipeline existed.
fn strong_etag(checksum: &str) -> String {
    format!(
        "\"{}\"",
        data_encoding::HEXLOWER.encode(&Sha256::digest(checksum.as_bytes()))
    )
}

fn matches_etag(header_value: &str, etag: &str) -> bool {
    header_value
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate.trim_start_matches("W/") == etag)
}

fn content_disposition(download: bool, file_name: &str) -> String {
    let fallback: String = file_name
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() && character != '"' && character != '\\' {
                character
            } else {
                '-'
            }
        })
        .take(120)
        .collect();
    let fallback = if fallback.trim_matches('-').is_empty() {
        "media".to_owned()
    } else {
        fallback
    };
    let kind = if download { "attachment" } else { "inline" };
    format!("{kind}; filename=\"{fallback}\"")
}
