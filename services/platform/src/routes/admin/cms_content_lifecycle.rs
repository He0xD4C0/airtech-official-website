async fn get_content_publication_readiness(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
) -> Result<Json<crate::models::ContentPublicationReadiness>, ApiError> {
    Ok(Json(
        cms_content::publication_readiness(
            &state,
            id,
            principal.has_permission("content.publish"),
        )
        .await?,
    ))
}

async fn publish_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<crate::models::PublishContentRequest>,
) -> Result<Response, ApiError> {
    if !principal.has_permission("content.publish") {
        return Err(ApiError::forbidden(
            "The `content.publish` permission is required.",
        ));
    }
    let expected = parse_content_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.v2.publish",
        &headers,
        &json!({"actor": actor_name, "id": id, "ifMatch": expected, "request": request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentRecordV2 = replay.decode()?;
            return Ok(cms_content_response(status, &entry));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let entry = cms_content::publish_content(
        &state,
        id,
        expected,
        request.reason,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::CREATED, &entry))
}

async fn archive_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<ArchiveContentRequest>,
) -> Result<Response, ApiError> {
    let expected = parse_content_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.v2.archive",
        &headers,
        &json!({
            "actor": actor_name,
            "id": id,
            "ifMatch": expected,
            "request": request,
        }),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentRecordV2 = replay.decode()?;
            return Ok(cms_content_response(status, &entry));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let entry = cms_content::archive_content(
        &state,
        id,
        expected,
        request.reason,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::OK, &entry))
}
