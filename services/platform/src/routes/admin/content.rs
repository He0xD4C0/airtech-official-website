async fn list_content(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<ContentEntry>>, ApiError> {
    let mut values = state.list_working_content().await?;
    // News has metadata and projection invariants that only the dedicated News
    // service can update transactionally. Keep it out of the generic editor.
    values.retain(|entry| entry.kind != ContentKind::News);
    values.sort_by_key(|entry| Reverse(entry.updated_at));
    Ok(Json(paginate_by_id(
        "admin.content",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn create_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ContentDraftInput>,
) -> Result<Response, ApiError> {
    reject_generic_news_input(input.kind)?;
    validate_content_input(&input)?;
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.create",
        &headers,
        &json!({"actor": &actor, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if state
        .content_identity_exists(input.kind, &input.slug, &input.locale, None)
        .await?
    {
        return Err(ApiError::conflict(
            "A content entry already uses this kind, slug and locale.",
        ));
    }
    let now = Utc::now();
    let entry = ContentEntry {
        id: Uuid::new_v4(),
        kind: input.kind,
        slug: input.slug,
        locale: input.locale,
        title: input.title,
        summary: input.summary,
        body: input.body,
        seo: input.seo,
        status: PublicationStatus::Draft,
        is_placeholder: input.is_placeholder,
        current_revision: 1,
        published_revision: None,
        scheduled_for: None,
        updated_at: now,
    };
    state.persist_content(&entry, &actor).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .content
            .insert(entry.id, entry.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "content.create",
        "content",
        Some(entry.id),
        None,
        Some(json!(entry)),
        Some("Create working draft".into()),
    )
    .await?;
    idempotency
        .complete(&state, &entry, StatusCode::CREATED)
        .await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &entry,
        entry.current_revision,
    ))
}

async fn update_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<ContentDraftInput>,
) -> Result<Response, ApiError> {
    reject_generic_news_input(input.kind)?;
    validate_content_input(&input)?;
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let before = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    reject_generic_news_entry(&before)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.update",
        &headers,
        &json!({
            "actor": &actor,
            "id": id,
            "ifMatch": expected_revision,
            "input": &input,
        }),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if before.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The content entry changed; reload before saving.",
        ));
    }
    if state
        .content_identity_exists(input.kind, &input.slug, &input.locale, Some(id))
        .await?
    {
        return Err(ApiError::conflict(
            "A content entry already uses this kind, slug and locale.",
        ));
    }
    let mut updated = before.clone();
    updated.kind = input.kind;
    updated.slug = input.slug;
    updated.locale = input.locale;
    updated.title = input.title;
    updated.summary = input.summary;
    updated.body = input.body;
    updated.seo = input.seo;
    updated.is_placeholder = input.is_placeholder;
    updated.status = PublicationStatus::Draft;
    updated.current_revision += 1;
    updated.updated_at = Utc::now();

    state
        .persist_content_update(&updated, &actor, expected_revision)
        .await?;
    if state.pool.is_none() {
        state.data.write().await.content.insert(id, updated.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "content.update",
        "content",
        Some(id),
        Some(json!(before)),
        Some(json!(updated)),
        Some("Update working draft".into()),
    )
    .await?;
    idempotency
        .complete(&state, &updated, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &updated,
        updated.current_revision,
    ))
}

async fn publish_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let before = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    reject_generic_news_entry(&before)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.publish",
        &headers,
        &json!({"actor": &actor, "id": id, "ifMatch": expected_revision}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if before.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The content entry changed; reload before publishing.",
        ));
    }
    if before.status == PublicationStatus::Published
        && before.published_revision == Some(before.current_revision)
    {
        idempotency
            .complete(&state, &before, StatusCode::OK)
            .await?;
        return Ok(entity_response(
            StatusCode::OK,
            &before,
            before.current_revision,
        ));
    }
    let mut published = before.clone();
    published.status = PublicationStatus::Published;
    published.published_revision = Some(published.current_revision);
    published.updated_at = Utc::now();
    if published.is_placeholder {
        published.seo.indexable = false;
    }
    state
        .publish_content_projection(&published, &actor, "publish", expected_revision)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .content
            .insert(id, published.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "content.publish",
        "content",
        Some(id),
        Some(json!(before)),
        Some(json!(published)),
        Some("Publish current content revision".into()),
    )
    .await?;
    idempotency
        .complete(&state, &published, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &published,
        published.current_revision,
    ))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackContentRequest {
    revision: i64,
    reason: String,
}

async fn rollback_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RollbackContentRequest>,
) -> Result<Response, ApiError> {
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let before = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    reject_generic_news_entry(&before)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.rollback",
        &headers,
        &json!({
            "actor": &actor,
            "id": id,
            "ifMatch": expected_revision,
            "request": &request,
        }),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if request.reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "Rollback reason must contain at least 10 characters.",
        ));
    }
    if before.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The content entry changed; reload before rolling back.",
        ));
    }
    let historic = state
        .load_content_revision(id, request.revision)
        .await?
        .ok_or_else(|| ApiError::not_found("The requested historical revision was not found."))?;

    // Rollback means re-publishing historic content as a new immutable
    // revision. The original history is never modified.
    let mut restored = historic;
    restored.current_revision = before.current_revision + 1;
    restored.published_revision = Some(restored.current_revision);
    restored.status = PublicationStatus::Published;
    restored.scheduled_for = None;
    restored.updated_at = Utc::now();
    if restored.is_placeholder {
        restored.seo.indexable = false;
    }
    state
        .publish_content_projection(&restored, &actor, "rollback", expected_revision)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .content
            .insert(id, restored.clone());
    }
    let rollback_reason = request.reason.clone();
    audit(
        &state,
        &headers,
        &actor,
        "content.rollback",
        "content",
        Some(id),
        Some(json!(before)),
        Some(json!({"published": restored, "sourceRevision": request.revision, "reason": request.reason})),
        Some(rollback_reason),
    )
    .await?;
    idempotency
        .complete(&state, &restored, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &restored,
        restored.current_revision,
    ))
}
