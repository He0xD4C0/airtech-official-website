use crate::{models::MediaAsset, services::media};

async fn upload_media_asset(
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

async fn get_media_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(asset_id): Path<Uuid>,
) -> Result<Json<MediaAsset>, ApiError> {
    require_media_read(&principal)?;
    let pool = crate::services::cms_content::require_postgres(&state)?;
    Ok(Json(
        crate::services::media_assets::load_media_asset(pool, asset_id).await?,
    ))
}

async fn list_media_references(
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

fn require_media_write(principal: &AdminPrincipal) -> Result<(), ApiError> {
    if principal.has_permission("media.write") {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "The `media.write` permission is required.",
        ))
    }
}

fn require_media_read(principal: &AdminPrincipal) -> Result<(), ApiError> {
    if principal.has_permission("media.write") || principal.has_permission("content.read") {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "The `content.read` or `media.write` permission is required.",
        ))
    }
}
