async fn list_operations(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<BackgroundOperation>>, ApiError> {
    let values = state.list_operations().await?;
    Ok(Json(paginate_by_id(
        "admin.operations",
        values,
        query,
        |operation| operation.id,
    )?))
}

async fn create_operation(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<CreateOperationRequest>,
) -> Result<Response, ApiError> {
    let actor = actor(&headers);
    if request.kind == OperationKind::MigrationApply {
        return Err(ApiError::conflict(
            "Schema migrations are deployment-only and must run through Flyway.",
        ));
    }
    let idempotency = match begin_idempotency(
        &state,
        "admin.operation.create",
        &headers,
        &json!({"actor": &actor, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let operation: BackgroundOperation = replay.decode()?;
            return Ok(operation_response(status, &operation));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if request.reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "Operation reason must contain at least 10 characters.",
        ));
    }
    let expected = confirmation_phrase(request.kind);
    if request.confirmation != expected {
        return Err(ApiError::bad_request(format!(
            "confirmation must exactly equal `{expected}`."
        )));
    }
    require_operation_totp(&state, &headers, &principal, request.kind).await?;
    let now = Utc::now();
    let operation = BackgroundOperation {
        id: Uuid::new_v4(),
        kind: request.kind,
        status: OperationStatus::Queued,
        reason: request.reason.trim().into(),
        created_at: now,
        updated_at: now,
        result: None,
    };
    state.persist_operation(&operation).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .operations
            .insert(operation.id, operation.clone());
    }
    let operation_reason = operation.reason.clone();
    audit(
        &state,
        &headers,
        &actor,
        "operation.queue",
        "operation",
        Some(operation.id),
        None,
        Some(json!(operation)),
        Some(operation_reason),
    )
    .await?;
    idempotency
        .complete(&state, &operation, StatusCode::ACCEPTED)
        .await?;
    Ok(operation_response(StatusCode::ACCEPTED, &operation))
}

fn operation_response(status: StatusCode, operation: &BackgroundOperation) -> Response {
    let mut response = (status, Json(operation)).into_response();
    response.headers_mut().insert(
        "location",
        HeaderValue::from_str(&format!("/api/admin/v1/operations/{}", operation.id))
            .expect("operation location is valid"),
    );
    response
}

async fn require_operation_totp(
    state: &AppState,
    headers: &HeaderMap,
    principal: &AdminPrincipal,
    kind: OperationKind,
) -> Result<(), ApiError> {
    if !matches!(
        kind,
        OperationKind::MigrationApply
            | OperationKind::Backup
            | OperationKind::RestoreValidate
            | OperationKind::RetentionApply
    ) {
        return Ok(());
    }
    if !principal
        .role
        .split(" · ")
        .any(|role| role == "Super Admin")
    {
        return Err(ApiError::forbidden(
            "A Super Admin role is required for high-risk operations.",
        ));
    }
    let code = headers
        .get("x-totp-code")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| ApiError::forbidden("A valid X-TOTP-Code is required."))?;
    crate::auth::verify_totp_reauthentication(state, principal, code).await
}

async fn get_operation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BackgroundOperation>, ApiError> {
    state
        .get_operation(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Operation was not found."))
}

async fn operation_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let operation = state
        .get_operation(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Operation was not found."))?;
    let updates = stream::unfold(
        (state, id, Some(operation), false),
        |(state, id, initial, finished)| async move {
            if finished {
                return None;
            }
            let operation = match initial {
                Some(operation) => Some(operation),
                None => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    state.get_operation(id).await.ok().flatten()
                }
            };
            let Some(operation) = operation else {
                let event = Event::default()
                    .event("error")
                    .data("Operation status is temporarily unavailable.");
                return Some((Ok(event), (state, id, None, true)));
            };
            let finished = matches!(
                operation.status,
                OperationStatus::Completed | OperationStatus::Failed
            );
            let event = Event::default()
                .event("operation")
                .json_data(&operation)
                .unwrap_or_else(|_| {
                    Event::default()
                        .event("error")
                        .data("Operation status serialization failed.")
                });
            Some((Ok(event), (state, id, None, finished)))
        },
    );
    Ok(Sse::new(updates).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("operation-stream"),
    ))
}
