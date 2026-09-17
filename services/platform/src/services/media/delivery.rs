use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

/// Compatibility endpoint for URLs emitted before direct external media URLs
/// were introduced. The immutable destination is stored with each asset.
pub async fn deliver_media_asset(
    state: &AppState,
    asset_id: Uuid,
    _download: bool,
    _request_headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let row = sqlx::query("SELECT public_url FROM media_assets WHERE id=$1 AND deleted_at IS NULL")
        .bind(asset_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Media asset was not found."))?;
    let public_url: Option<String> = row.try_get("public_url")?;
    let public_url = public_url.ok_or_else(|| {
        ApiError::service_unavailable(
            "This legacy media asset has not been adopted into object storage settings.",
        )
    })?;
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
