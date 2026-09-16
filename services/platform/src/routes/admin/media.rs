use super::*;

pub(super) use crate::{
    models::MediaAsset,
    services::{
        media,
        media_assets::{self, MediaAssetFilter},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MediaAssetListQuery {
    pub(super) cursor: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) q: Option<String>,
}

pub(super) async fn list_media_assets(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(query): Query<MediaAssetListQuery>,
) -> Result<Json<media_assets::MediaAssetPage>, ApiError> {
    require_media_read(&principal)?;
    let filter = MediaAssetFilter::parse(query.q)?;
    Ok(Json(
        media_assets::list_media_assets(
            &state,
            filter,
            CursorQuery {
                cursor: query.cursor,
                limit: query.limit,
            },
        )
        .await?,
    ))
}

pub(super) async fn upload_media_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, ApiError> {
    require_media_write(&principal)?;
    let idempotency_key = crate::idempotency::idempotency_key(&headers)?;
    let asset = media::upload_media_asset(
        &state,
        &principal.email,
        Uuid::new_v4(),
        &idempotency_key,
        multipart,
    )
    .await?;
    Ok(crate::routes::accepted(StatusCode::CREATED, asset))
}

pub(super) async fn get_media_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(asset_id): Path<Uuid>,
) -> Result<Json<MediaAsset>, ApiError> {
    require_media_read(&principal)?;
    let pool = &state.pool;
    Ok(Json(
        crate::services::media_assets::load_media_asset(pool, asset_id).await?,
    ))
}

pub(super) async fn list_media_references(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(asset_id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::MediaAssetReference>>, ApiError> {
    require_media_read(&principal)?;
    Ok(Json(
        crate::services::media_assets::list_media_references(&state, asset_id, query).await?,
    ))
}

pub(super) fn require_media_write(principal: &AdminPrincipal) -> Result<(), ApiError> {
    if principal.has_permission("media.write") {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "The `media.write` permission is required.",
        ))
    }
}

pub(super) fn require_media_read(principal: &AdminPrincipal) -> Result<(), ApiError> {
    if principal.has_permission("media.write") || principal.has_permission("content.read") {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "The `content.read` or `media.write` permission is required.",
        ))
    }
}
