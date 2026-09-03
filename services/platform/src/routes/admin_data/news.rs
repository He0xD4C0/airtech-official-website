// News is a revisioned ContentKind::News document with revision-specific list metadata.
async fn list_news(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<NewsEntry>>, ApiError> {
    let mut values = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT entry.payload, news.category, news.author_display_name,
                      news.cover_media_asset_id, news.publication_at, news.featured,
                      news.data_origin
               FROM content_entries entry
               JOIN news_working news ON news.content_id=entry.id
               WHERE entry.kind='news' ORDER BY entry.updated_at DESC"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(decode_news_row)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        state.data.read().await.news.values().cloned().collect()
    };
    values.sort_by_key(|entry| std::cmp::Reverse(entry.content.updated_at));
    Ok(Json(paginate_by_id(
        "admin.news",
        values,
        query,
        |entry| entry.content.id,
    )?))
}

async fn get_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let entry = load_news(&state, id).await?;
    Ok(entity_response(
        StatusCode::OK,
        &entry,
        entry.content.current_revision,
    ))
}

async fn list_news_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CursorPage<NewsEntry>>, ApiError> {
    // Resolve the working record first so an unknown id cannot be confused
    // with an entry that simply has no immutable snapshots yet.
    load_news(&state, id).await?;
    let mut revisions = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT content_revision.payload,news.category,news.author_display_name,
                      news.cover_media_asset_id,news.publication_at,news.featured,
                      news.data_origin
               FROM content_revisions content_revision
               JOIN content_entries entry ON entry.id=content_revision.content_id
               JOIN news ON news.content_id=content_revision.content_id
                 AND news.revision=content_revision.revision
               WHERE content_revision.content_id=$1 AND entry.kind='news'
               ORDER BY content_revision.revision DESC"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(decode_news_row)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let data = state.data.read().await;
        let metadata = data.news.get(&id).cloned();
        data.content_revisions
            .get(&id)
            .into_iter()
            .flat_map(|values| values.values())
            .filter_map(|content| {
                metadata.clone().map(|metadata| NewsEntry {
                    content: content.clone(),
                    ..metadata
                })
            })
            .collect()
    };
    revisions.sort_by_key(|entry| std::cmp::Reverse(entry.content.current_revision));
    Ok(Json(CursorPage::all(revisions)))
}

async fn create_news(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut input): Json<NewsDraftInput>,
) -> Result<Response, ApiError> {
    normalize_news_ownership(state.config.production, None, &mut input)?;
    validate_news_input(&mut input)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.news.create",
        &headers,
        &json!({"actor": &actor_name, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let now = Utc::now();
    let entry = NewsEntry {
        content: ContentEntry {
            id: Uuid::new_v4(),
            kind: ContentKind::News,
            slug: input.content.slug,
            locale: input.content.locale,
            title: input.content.title,
            summary: input.content.summary,
            body: input.content.body,
            seo: input.content.seo,
            status: PublicationStatus::Draft,
            is_placeholder: input.content.is_placeholder,
            current_revision: 1,
            published_revision: None,
            scheduled_for: None,
            updated_at: now,
        },
        category: input.category,
        author_display_name: Some(input.author_display_name),
        cover_media_id: input.cover_media_id,
        published_at: input.published_at,
        featured: input.featured,
        data_class: input.data_class,
    };
    let audit = mutation_audit_event(
        &headers,
        "news.create",
        "news",
        Some(entry.content.id),
        None,
        Some(json!(entry)),
        Some("Create news working draft".into()),
    );
    persist_news_revision(&state, &entry, None, false, &actor_name, &audit).await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content.insert(entry.content.id, entry.content.clone());
        data.news.insert(entry.content.id, entry.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &entry, StatusCode::CREATED)
        .await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &entry,
        entry.content.current_revision,
    ))
}

async fn update_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(mut input): Json<NewsDraftInput>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let before = load_news(&state, id).await?;
    normalize_news_ownership(state.config.production, Some(before.data_class), &mut input)?;
    validate_news_input(&mut input)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.news.update",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if before.content.current_revision != expected {
        return Err(ApiError::conflict(
            "The news entry changed; reload before saving.",
        ));
    }
    let mut updated = before.clone();
    updated.content.slug = input.content.slug;
    updated.content.locale = input.content.locale;
    updated.content.title = input.content.title;
    updated.content.summary = input.content.summary;
    updated.content.body = input.content.body;
    updated.content.seo = input.content.seo;
    updated.content.is_placeholder = input.content.is_placeholder;
    updated.content.status = PublicationStatus::Draft;
    updated.content.current_revision += 1;
    updated.content.updated_at = Utc::now();
    updated.category = input.category;
    updated.author_display_name = Some(input.author_display_name);
    updated.cover_media_id = input.cover_media_id;
    updated.published_at = input.published_at;
    updated.featured = input.featured;
    updated.data_class = input.data_class;
    let audit = mutation_audit_event(
        &headers,
        "news.update",
        "news",
        Some(id),
        Some(json!(before)),
        Some(json!(updated)),
        Some("Update news working draft".into()),
    );
    persist_news_revision(&state, &updated, Some(expected), false, &actor_name, &audit).await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content.insert(id, updated.content.clone());
        data.news.insert(id, updated.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &updated, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &updated,
        updated.content.current_revision,
    ))
}

async fn publish_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.news.publish",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_news(&state, id).await?;
    if before.content.current_revision != expected {
        return Err(ApiError::conflict(
            "The news entry changed; reload before publishing.",
        ));
    }
    let mut published = before.clone();
    published.content.status = PublicationStatus::Published;
    published.content.published_revision = Some(expected);
    published.content.updated_at = Utc::now();
    if published.content.is_placeholder || published.data_class == DataClass::DevelopmentFixture {
        published.content.seo.indexable = false;
    }
    let audit = mutation_audit_event(
        &headers,
        "news.publish",
        "news",
        Some(id),
        Some(json!(before)),
        Some(json!(published)),
        Some("Publish current news revision".into()),
    );
    persist_news_revision(
        &state,
        &published,
        Some(expected),
        true,
        &actor_name,
        &audit,
    )
    .await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content_revisions
            .entry(id)
            .or_default()
            .insert(expected, published.content.clone());
        data.content.insert(id, published.content.clone());
        data.published_content.insert(id, published.content.clone());
        data.news.insert(id, published.clone());
        data.published_news.insert(id, published.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &published, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &published,
        published.content.current_revision,
    ))
}

async fn rollback_news(
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
        "admin.news.rollback",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_news(&state, id).await?;
    if before.content.current_revision != expected {
        return Err(ApiError::conflict(
            "The news entry changed; reload before rolling back.",
        ));
    }
    let mut restored = load_news_revision(&state, id, request.revision).await?;
    // Fixture ownership is one-way. Republishing a historic fixture snapshot
    // after editorial takeover must not hand the deterministic record back to
    // the development seed.
    if before.data_class == DataClass::Editorial || state.config.production {
        restored.data_class = DataClass::Editorial;
    }
    restored.content.current_revision = expected + 1;
    restored.content.published_revision = Some(expected + 1);
    restored.content.status = PublicationStatus::Published;
    restored.content.updated_at = Utc::now();
    if restored.content.is_placeholder || restored.data_class == DataClass::DevelopmentFixture {
        restored.content.seo.indexable = false;
    }
    let audit = mutation_audit_event(
        &headers,
        "news.rollback",
        "news",
        Some(id),
        Some(json!(before)),
        Some(json!(restored)),
        Some(request.reason.clone()),
    );
    persist_news_revision(&state, &restored, Some(expected), true, &actor_name, &audit).await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content_revisions
            .entry(id)
            .or_default()
            .insert(restored.content.current_revision, restored.content.clone());
        data.content.insert(id, restored.content.clone());
        data.published_content.insert(id, restored.content.clone());
        data.news.insert(id, restored.clone());
        data.published_news.insert(id, restored.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &restored, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &restored,
        restored.content.current_revision,
    ))
}
