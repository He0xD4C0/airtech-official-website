use super::*;

pub(super) async fn list_temporary_overrides(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<TemporaryOverride>>, ApiError> {
    Ok(Json(
        crate::services::admin_products::list_temporary_overrides(&state, id, query).await?,
    ))
}

pub(super) async fn create_temporary_override(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(mut input): Json<CreateTemporaryOverride>,
) -> Result<Response, ApiError> {
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.product.override.create",
        &headers,
        &json!({"actor": &actor, "id": id, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let value: TemporaryOverride = replay.decode()?;
            return Ok((status, Json(value)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if input.product_id != id {
        return Err(ApiError::bad_request(
            "Path product id must match productId.",
        ));
    }
    if state.load_working_product(id).await?.is_none() {
        return Err(ApiError::not_found("Product was not found."));
    }
    if input.reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "Override reason must contain at least 10 characters.",
        ));
    }
    if input.field_path.trim().is_empty() || input.field_path.starts_with("site.") {
        return Err(ApiError::bad_request(
            "fieldPath must identify a Feishu-owned source field; site-owned fields do not require overrides.",
        ));
    }
    if let Some(expires_at) = input.expires_at {
        if expires_at <= Utc::now() {
            return Err(ApiError::bad_request("expiresAt must be in the future."));
        }
    } else {
        let default_days = state
            .integer_setting("temporaryOverrideDefaultDays", 30, 1, 365)
            .await?;
        input.expires_at = Some(Utc::now() + ChronoDuration::days(default_days));
    }
    input.reason = input.reason.trim().to_owned();
    let value = TemporaryOverride::from_input(input);
    state.persist_override(&value).await?;
    let override_reason = value.reason.clone();
    audit(
        &state,
        &headers,
        &actor,
        "product.override.create",
        "temporaryOverride",
        Some(value.id),
        None,
        Some(json!(value)),
        Some(override_reason),
    )
    .await?;
    idempotency
        .complete(&state, &value, StatusCode::CREATED)
        .await?;
    Ok((StatusCode::CREATED, Json(value)).into_response())
}
