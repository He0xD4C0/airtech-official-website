use super::*;

pub(super) async fn get_settings(State(state): State<AppState>) -> Result<Response, ApiError> {
    let settings = state.platform_settings().await?;
    let revision = settings.revision;
    let mut response = entity_response(StatusCode::OK, &settings, revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdatePlatformSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(update) = payload.map_err(|_| {
        ApiError::bad_request(
            "Settings must be a valid JSON object containing only the documented mutable fields and reason.",
        )
    })?;
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    let settings = state
        .update_platform_settings(expected_revision, &update, &actor, request_id)
        .await?;
    let revision = settings.revision;
    let mut response = entity_response(StatusCode::OK, &settings, revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn get_object_storage_settings(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let settings = crate::services::object_storage_settings::get(&state).await?;
    let mut response = entity_response(StatusCode::OK, &settings, settings.revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn test_object_storage_settings(
    State(state): State<AppState>,
    payload: Result<Json<ObjectStorageSettingsInput>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = payload.map_err(|_| {
        ApiError::bad_request("Object storage settings must use the documented JSON shape.")
    })?;
    let result = crate::services::object_storage_settings::test(&state, &input).await?;
    let mut response = (StatusCode::OK, Json(result)).into_response();
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn update_object_storage_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdateObjectStorageSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(update) = payload.map_err(|_| {
        ApiError::bad_request("Object storage settings must use the documented JSON shape.")
    })?;
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    let settings = crate::services::object_storage_settings::update(
        &state,
        expected_revision,
        &update,
        &actor,
        request_id,
    )
    .await?;
    let mut response = entity_response(StatusCode::OK, &settings, settings.revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}
