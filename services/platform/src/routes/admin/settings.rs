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
