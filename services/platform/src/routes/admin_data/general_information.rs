// General Information is one locale-aware revisioned site aggregate.
async fn get_general_information(
    State(state): State<AppState>,
    Query(query): Query<LocaleQuery>,
) -> Result<Response, ApiError> {
    let entry = load_general_information_by_locale(&state, &query.locale)
        .await?
        .ok_or_else(|| ApiError::not_found("General Information was not found."))?;
    Ok(entity_response(
        StatusCode::OK,
        &entry,
        entry.current_revision,
    ))
}

async fn get_general_information_by_id(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let entry = load_general_information(&state, id).await?;
    Ok(entity_response(
        StatusCode::OK,
        &entry,
        entry.current_revision,
    ))
}

async fn list_general_information_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CursorPage<GeneralInformation>>, ApiError> {
    let current = load_general_information(&state, id).await?;
    let mut revisions = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT information.id,revision.locale,revision.payload,
                      information.published_revision,revision.is_placeholder,
                      revision.revision,revision.created_at
               FROM general_information_revisions revision
               JOIN general_information information
                 ON information.id=revision.general_information_id
               WHERE revision.general_information_id=$1
               ORDER BY revision.revision DESC"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let revision: i64 = row.try_get("revision")?;
                let published_revision: Option<i64> = row.try_get("published_revision")?;
                Ok::<GeneralInformation, ApiError>(GeneralInformation {
                    id: row.try_get("id")?,
                    locale: row.try_get("locale")?,
                    payload: row.try_get("payload")?,
                    status: if published_revision == Some(revision) {
                        PublicationStatus::Published
                    } else {
                        PublicationStatus::Draft
                    },
                    current_revision: revision,
                    published_revision: (published_revision == Some(revision)).then_some(revision),
                    is_placeholder: row.try_get("is_placeholder")?,
                    updated_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        state
            .data
            .read()
            .await
            .general_information_revisions
            .get(&id)
            .map(|values| values.values().cloned().collect())
            .unwrap_or_default()
    };
    revisions.sort_by_key(|entry| std::cmp::Reverse(entry.current_revision));
    // Retain the current read above so future query changes do not accidentally
    // turn this endpoint into a cross-tenant revision lookup.
    debug_assert_eq!(current.id, id);
    Ok(Json(CursorPage::all(revisions)))
}

async fn create_general_information(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<GeneralInformationDraftInput>,
) -> Result<Response, ApiError> {
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.create",
        &headers,
        &json!({"actor": &actor_name, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_general_information(&input)?;
    if load_general_information_by_locale(&state, &input.locale)
        .await?
        .is_some()
    {
        return Err(ApiError::conflict(
            "General Information already exists for this locale.",
        ));
    }
    let entry = GeneralInformation {
        id: Uuid::new_v4(),
        locale: input.locale,
        payload: input.payload,
        status: PublicationStatus::Draft,
        current_revision: 1,
        published_revision: None,
        is_placeholder: input.is_placeholder,
        updated_at: Utc::now(),
    };
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.create",
        "generalInformation",
        Some(entry.id),
        None,
        Some(json!(entry)),
        Some("Create General Information draft".into()),
    );
    persist_general_information_revision(&state, &entry, None, false, &actor_name, &audit, None)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .general_information
            .insert(entry.id, entry.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &entry, StatusCode::CREATED)
        .await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &entry,
        entry.current_revision,
    ))
}

async fn update_general_information(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<GeneralInformationDraftInput>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.update",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_general_information(&input)?;
    let before = load_general_information(&state, id).await?;
    if input.locale != before.locale {
        return Err(ApiError::validation(BTreeMap::from([(
            "locale".into(),
            vec!["Locale is immutable after General Information is created.".into()],
        )])));
    }
    if before.current_revision != expected {
        return Err(ApiError::conflict(
            "General Information changed; reload before saving.",
        ));
    }
    let mut updated = before.clone();
    updated.locale = input.locale;
    updated.payload = input.payload;
    updated.is_placeholder = input.is_placeholder;
    updated.status = PublicationStatus::Draft;
    updated.current_revision += 1;
    updated.updated_at = Utc::now();
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.update",
        "generalInformation",
        Some(id),
        Some(json!(before)),
        Some(json!(updated)),
        Some("Update General Information draft".into()),
    );
    persist_general_information_revision(
        &state,
        &updated,
        Some(expected),
        false,
        &actor_name,
        &audit,
        None,
    )
    .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .general_information
            .insert(id, updated.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &updated, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &updated,
        updated.current_revision,
    ))
}

async fn publish_general_information(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.publish",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_general_information(&state, id).await?;
    if before.current_revision != expected {
        return Err(ApiError::conflict(
            "General Information changed; reload before publishing.",
        ));
    }
    let mut published = before.clone();
    published.status = PublicationStatus::Published;
    published.published_revision = Some(expected);
    published.updated_at = Utc::now();
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.publish",
        "generalInformation",
        Some(id),
        Some(json!(before)),
        Some(json!(published)),
        Some("Publish General Information revision".into()),
    );
    persist_general_information_revision(
        &state,
        &published,
        Some(expected),
        true,
        &actor_name,
        &audit,
        Some("publish"),
    )
    .await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.general_information_revisions
            .entry(id)
            .or_default()
            .insert(published.current_revision, published.clone());
        data.general_information.insert(id, published.clone());
        data.published_general_information
            .insert(id, published.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &published, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &published,
        published.current_revision,
    ))
}

async fn rollback_general_information(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RevisionRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.rollback",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_general_information(&state, id).await?;
    if before.current_revision != expected {
        return Err(ApiError::conflict(
            "General Information changed; reload before rolling back.",
        ));
    }
    let mut restored = load_general_information_revision(&state, id, request.revision).await?;
    restored.current_revision = expected + 1;
    restored.published_revision = Some(expected + 1);
    restored.status = PublicationStatus::Published;
    restored.updated_at = Utc::now();
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.rollback",
        "generalInformation",
        Some(id),
        Some(json!(before)),
        Some(json!(restored)),
        Some(request.reason.clone()),
    );
    persist_general_information_revision(
        &state,
        &restored,
        Some(expected),
        true,
        &actor_name,
        &audit,
        Some("rollback"),
    )
    .await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.general_information_revisions
            .entry(id)
            .or_default()
            .insert(restored.current_revision, restored.clone());
        data.general_information.insert(id, restored.clone());
        data.published_general_information
            .insert(id, restored.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &restored, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &restored,
        restored.current_revision,
    ))
}
