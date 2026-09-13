use crate::models::{
    CmsContentKind, CmsPublicationStatusV2, ContentTemplateDefinition,
};
use crate::pagination::paginate_by_id_scoped;
use crate::services::cms_content::{ContentListFilter, ContentSortField, SortDirection};
use crate::services::cms_templates;
use crate::services::media_assets::{self, MediaAssetFilter};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentDiffQuery {
    base_revision: i64,
    target_revision: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaAssetListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
}

async fn list_media_assets(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(query): Query<MediaAssetListQuery>,
) -> Result<Json<media_assets::MediaAssetPage>, ApiError> {
    require_media_read(&principal)?;
    let filter = MediaAssetFilter::parse(query.q)?;
    let page = media_assets::list_media_assets(
        &state,
        filter,
        CursorQuery {
            cursor: query.cursor,
            limit: query.limit,
        },
    )
    .await?;
    Ok(Json(page))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
    kind: Option<String>,
    status: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
}

impl ContentListQuery {
    fn into_parts(self) -> Result<(ContentListFilter, CursorQuery), ApiError> {
        let query = parse_query_text(self.q)?;
        let kinds = parse_kinds(self.kind)?;
        let status = parse_status(self.status)?;
        let sort = parse_sort(self.sort)?;
        let direction = parse_direction(self.direction, sort)?;
        Ok((
            ContentListFilter {
                query,
                kinds,
                status,
                sort,
                direction,
            },
            CursorQuery {
                cursor: self.cursor,
                limit: self.limit,
            },
        ))
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentRecordListPage {
    items: Vec<ContentRecordV2>,
    next_cursor: Option<String>,
    total: usize,
    counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentTemplateListPage {
    items: Vec<ContentTemplateDefinition>,
}

async fn list_content(
    State(state): State<AppState>,
    Query(query): Query<ContentListQuery>,
) -> Result<Json<ContentRecordListPage>, ApiError> {
    let (filter, pagination) = query.into_parts()?;
    let outcome = cms_content::list_content(&state, filter.clone()).await?;
    let scope = format!("admin.content.v2|{}", filter.cursor_scope());
    let pagination_started = std::time::Instant::now();
    let page = paginate_by_id_scoped(&scope, outcome.records, pagination, |entry| entry.id)?;
    let pagination_elapsed = pagination_started.elapsed();
    let response = ContentRecordListPage {
        items: page.items,
        next_cursor: page.next_cursor,
        total: outcome.total,
        counts: outcome.counts,
    };
    let estimated_payload_bytes = serde_json::to_vec(&response)
        .map_err(|_| ApiError::internal("Unable to measure the content list response."))?
        .len();
    crate::services::list_filter_observability::observe_list(
        "admin.cms_content",
        outcome.observation,
        pagination_elapsed,
        response.items.len(),
        estimated_payload_bytes,
    );
    Ok(Json(response))
}

async fn list_content_templates() -> Result<Json<ContentTemplateListPage>, ApiError> {
    Ok(Json(ContentTemplateListPage {
        items: cms_templates::template_registry().to_vec(),
    }))
}

fn parse_query_text(value: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > 200 {
        return Err(ApiError::bad_request("q must be at most 200 characters."));
    }
    Ok(Some(trimmed.to_string()))
}

fn parse_kinds(value: Option<String>) -> Result<Vec<CmsContentKind>, ApiError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let mut kinds = Vec::new();
    for raw in value.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let kind: CmsContentKind = serde_json::from_value(Value::String(entry.to_string()))
            .map_err(|_| {
                ApiError::bad_request("kind must be a comma-separated list of CMS content kinds.")
            })?;
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    Ok(kinds)
}

fn parse_status(value: Option<String>) -> Result<Option<CmsPublicationStatusV2>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    serde_json::from_value(Value::String(trimmed.to_string()))
        .map(Some)
        .map_err(|_| ApiError::bad_request("status must be draft, published, or archived."))
}

fn parse_sort(value: Option<String>) -> Result<ContentSortField, ApiError> {
    match value.as_deref() {
        None | Some("") => Ok(ContentSortField::UpdatedAt),
        Some("updatedAt") => Ok(ContentSortField::UpdatedAt),
        Some("title") => Ok(ContentSortField::Title),
        Some("kind") => Ok(ContentSortField::Kind),
        Some(_) => Err(ApiError::bad_request("sort must be updatedAt, title, or kind.")),
    }
}

fn parse_direction(
    value: Option<String>,
    sort: ContentSortField,
) -> Result<SortDirection, ApiError> {
    match value.as_deref() {
        None | Some("") => Ok(default_direction(sort)),
        Some("asc") => Ok(SortDirection::Asc),
        Some("desc") => Ok(SortDirection::Desc),
        Some(_) => Err(ApiError::bad_request("direction must be asc or desc.")),
    }
}

fn default_direction(sort: ContentSortField) -> SortDirection {
    match sort {
        ContentSortField::Title | ContentSortField::Kind => SortDirection::Asc,
        ContentSortField::UpdatedAt => SortDirection::Desc,
    }
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
    headers: HeaderMap,
    Json(request): Json<CreateContentSnapshotRequest>,
) -> Result<Response, ApiError> {
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
        request.reason,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::CREATED, &entry))
}

async fn unpublish_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<UnpublishContentRequest>,
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
        "admin.content.v2.unpublish",
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
    let entry = cms_content::unpublish_content(
        &state,
        id,
        expected,
        request.expected_published_revision,
        request.reason,
        mutation_metadata(&headers, actor_name),
        idempotency,
    )
    .await?;
    Ok(cms_content_response(StatusCode::OK, &entry))
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
