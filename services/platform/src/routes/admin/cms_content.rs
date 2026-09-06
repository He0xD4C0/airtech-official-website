#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentDiffQuery {
    base_revision: i64,
    target_revision: Option<i64>,
}

async fn list_content(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<ContentRecordV2>>, ApiError> {
    let values = cms_content::list_content(&state).await?;
    Ok(Json(paginate_by_id(
        "admin.content.v2",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn get_content_draft(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let entry = cms_content::get_content(&state, id).await?;
    Ok(cms_content_response(StatusCode::OK, &entry))
}

async fn create_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(draft): Json<ContentDraftV2>,
) -> Result<Response, ApiError> {
    let expected = parse_content_if_match(&headers)?;
    if expected != 0 {
        return Err(ApiError::conflict(
            "New content requires If-Match draft-0.",
        ));
    }
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.v2.create",
        &headers,
        &json!({"actor": actor_name, "ifMatch": expected, "draft": draft}),
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
    let entry = cms_content::create_content(
        &state,
        draft,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::CREATED, &entry))
}

async fn update_content_draft(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(draft): Json<ContentDraftV2>,
) -> Result<Response, ApiError> {
    let expected = parse_content_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.v2.draft.save",
        &headers,
        &json!({"actor": actor_name, "id": id, "ifMatch": expected, "draft": draft}),
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
    let entry = cms_content::save_draft(
        &state,
        id,
        expected,
        draft,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::OK, &entry))
}

async fn create_content_snapshot(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<CreateContentSnapshotRequest>,
) -> Result<Response, ApiError> {
    if request.intent == ContentSnapshotIntent::Publish
        && !principal.has_permission("content.publish")
    {
        return Err(ApiError::forbidden(
            "The `content.publish` permission is required.",
        ));
    }
    let expected = parse_content_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.v2.snapshot",
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
    let entry = cms_content::snapshot_content(
        &state,
        id,
        expected,
        request.intent,
        request.reason,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::CREATED, &entry))
}

async fn list_content_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<ContentRevisionV2>>, ApiError> {
    let values = cms_content::list_revisions(&state, id).await?;
    Ok(Json(paginate_by_id(
        "admin.content.v2.revisions",
        values,
        query,
        |entry| Uuid::from_u128(entry.revision as u128),
    )?))
}

async fn get_content_diff(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ContentDiffQuery>,
) -> Result<Json<crate::models::ContentDiffV2>, ApiError> {
    Ok(Json(
        cms_content::diff_content(&state, id, query.base_revision, query.target_revision).await?,
    ))
}

async fn restore_content_revision(
    State(state): State<AppState>,
    Path((id, revision)): Path<(Uuid, i64)>,
    headers: HeaderMap,
    Json(request): Json<RestoreContentRevisionRequest>,
) -> Result<Response, ApiError> {
    let expected = parse_content_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.v2.revision.restore",
        &headers,
        &json!({
            "actor": actor_name,
            "id": id,
            "revision": revision,
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
    let entry = cms_content::restore_revision(
        &state,
        id,
        revision,
        expected,
        request.reason,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::CREATED, &entry))
}

fn parse_content_if_match(headers: &HeaderMap) -> Result<i64, ApiError> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .and_then(cms_content::parse_draft_etag)
        .ok_or_else(|| {
            ApiError::precondition_required(
                "If-Match is required and must use the current draft-N ETag.",
            )
        })
}

fn cms_content_response(status: StatusCode, value: &ContentRecordV2) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&cms_content::draft_etag(value.draft.draft_version))
            .expect("draft ETag is valid"),
    );
    response
}

fn mutation_metadata(headers: &HeaderMap, actor: String) -> MutationMetadata {
    MutationMetadata {
        actor,
        request_id: headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(Uuid::new_v4),
    }
}
