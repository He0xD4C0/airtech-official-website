use crate::services::media;

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
