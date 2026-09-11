use crate::services::media;

/// Immutable public delivery for reviewed media objects.
///
/// The response is cacheable forever because a media asset id never changes
/// bytes: replacement means a new row with a new id.
async fn get_public_media(
    State(state): State<AppState>,
    Path(asset_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    media::deliver_media_asset(&state, asset_id, false, &headers).await
}

async fn download_public_media(
    State(state): State<AppState>,
    Path(asset_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    media::deliver_media_asset(&state, asset_id, true, &headers).await
}
