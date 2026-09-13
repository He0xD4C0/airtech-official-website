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
