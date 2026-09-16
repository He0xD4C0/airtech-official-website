use super::*;

pub(super) use crate::services::media;

pub(super) async fn get_public_media(
    State(state): State<AppState>,
    Path(asset_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    media::deliver_media_asset(&state, asset_id, false, &headers).await
}

pub(super) async fn download_public_media(
    State(state): State<AppState>,
    Path(asset_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    media::deliver_media_asset(&state, asset_id, true, &headers).await
}
