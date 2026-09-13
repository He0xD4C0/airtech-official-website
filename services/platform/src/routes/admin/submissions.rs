use crate::{
    models::{
        AssignBusinessInboxRequest, BusinessEntityType, BusinessInboxDetail, BusinessInboxItem,
        BusinessInboxPage, BusinessInboxStatus, BusinessPii, CreateBusinessNoteRequest,
        UpdateBusinessStatusRequest,
    },
    services::business_inbox::{self, InboxFilter},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BusinessInboxQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
    status: Option<String>,
    assigned_to: Option<Uuid>,
}

async fn list_rfqs(
    State(state): State<AppState>,
    Query(query): Query<BusinessInboxQuery>,
) -> Result<Json<BusinessInboxPage>, ApiError> {
    list_business(&state, BusinessEntityType::Rfq, query).await
}

async fn list_contacts(
    State(state): State<AppState>,
    Query(query): Query<BusinessInboxQuery>,
) -> Result<Json<BusinessInboxPage>, ApiError> {
    list_business(&state, BusinessEntityType::Contact, query).await
}

async fn list_business(
    state: &AppState,
    entity_type: BusinessEntityType,
    query: BusinessInboxQuery,
) -> Result<Json<BusinessInboxPage>, ApiError> {
    let filter = InboxFilter {
        query: parse_query_text(query.q)?,
        status: query.status.map(parse_business_status).transpose()?,
        assigned_to: query.assigned_to,
    };
    let values = business_inbox::list(state, entity_type, &filter).await?;
    let total = values.len();
    let scope = format!(
        "admin.business.{}|{:?}|{:?}|{:?}",
        entity_type.label(),
        filter.query,
        filter.status,
        filter.assigned_to
    );
    let page = crate::pagination::paginate_by_id_scoped(
        &scope,
        values,
        CursorQuery {
            cursor: query.cursor,
            limit: query.limit,
        },
        |item| item.id,
    )?;
    Ok(Json(BusinessInboxPage {
        items: page.items,
        next_cursor: page.next_cursor,
        total,
    }))
}

async fn get_rfq(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    business_detail_response(&state, BusinessEntityType::Rfq, id).await
}

async fn get_contact(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    business_detail_response(&state, BusinessEntityType::Contact, id).await
}

async fn business_detail_response(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
) -> Result<Response, ApiError> {
    let detail = business_inbox::detail(state, entity_type, id).await?;
    Ok(entity_response(
        StatusCode::OK,
        &detail,
        detail.item.revision,
    ))
}

async fn get_rfq_pii(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    pii_response(&state, BusinessEntityType::Rfq, id, &principal, &headers).await
}

async fn get_contact_pii(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    pii_response(
        &state,
        BusinessEntityType::Contact,
        id,
        &principal,
        &headers,
    )
    .await
}

async fn pii_response(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
    principal: &AdminPrincipal,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    if !principal.has_permission("rfq.read_pii") {
        return Err(ApiError::forbidden(
            "The `rfq.read_pii` permission is required.",
        ));
    }
    let pii = business_inbox::pii(state, entity_type, id).await?;
    audit(
        state,
        headers,
        &principal.email,
        "business.pii.read",
        entity_type.label(),
        Some(id),
        None,
        None,
        Some("Explicit PII detail read".into()),
    )
    .await?;
    let mut response = (StatusCode::OK, Json::<BusinessPii>(pii)).into_response();
    add_private_no_store_headers(&mut response);
    Ok(response)
}

async fn assign_rfq(
    state: State<AppState>,
    path: Path<Uuid>,
    principal: Extension<AdminPrincipal>,
    headers: HeaderMap,
    request: Json<AssignBusinessInboxRequest>,
) -> Result<Response, ApiError> {
    assign_business(
        state,
        path,
        principal,
        headers,
        request,
        BusinessEntityType::Rfq,
    )
    .await
}

async fn assign_contact(
    state: State<AppState>,
    path: Path<Uuid>,
    principal: Extension<AdminPrincipal>,
    headers: HeaderMap,
    request: Json<AssignBusinessInboxRequest>,
) -> Result<Response, ApiError> {
    assign_business(
        state,
        path,
        principal,
        headers,
        request,
        BusinessEntityType::Contact,
    )
    .await
}

async fn assign_business(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<AssignBusinessInboxRequest>,
    entity_type: BusinessEntityType,
) -> Result<Response, ApiError> {
    require_business_mutation(&principal)?;
    let expected = parse_if_match(&headers)?;
    let idempotency = match begin_item_idempotency(
        &state,
        entity_type,
        "assign",
        id,
        expected,
        &request,
        &principal,
        &headers,
    )
    .await?
    {
        ItemIdempotency::Replay(response) => return Ok(response),
        ItemIdempotency::Fresh(context) => context,
    };
    let before = business_inbox::get(&state, entity_type, id).await?;
    let item = business_inbox::assign(
        &state,
        entity_type,
        id,
        expected,
        request.assigned_to,
        &request.reason,
        &principal.email,
    )
    .await?;
    audit_business_mutation(
        &state,
        &headers,
        &principal,
        "business.assign",
        entity_type,
        id,
        &before,
        &item,
        &request.reason,
    )
    .await?;
    idempotency
        .complete(&state, &item, StatusCode::OK)
        .await?;
    Ok(entity_response(StatusCode::OK, &item, item.revision))
}

async fn update_rfq_status(
    state: State<AppState>,
    path: Path<Uuid>,
    principal: Extension<AdminPrincipal>,
    headers: HeaderMap,
    request: Json<UpdateBusinessStatusRequest>,
) -> Result<Response, ApiError> {
    update_business_status(
        state,
        path,
        principal,
        headers,
        request,
        BusinessEntityType::Rfq,
    )
    .await
}

async fn update_contact_status(
    state: State<AppState>,
    path: Path<Uuid>,
    principal: Extension<AdminPrincipal>,
    headers: HeaderMap,
    request: Json<UpdateBusinessStatusRequest>,
) -> Result<Response, ApiError> {
    update_business_status(
        state,
        path,
        principal,
        headers,
        request,
        BusinessEntityType::Contact,
    )
    .await
}

async fn update_business_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<UpdateBusinessStatusRequest>,
    entity_type: BusinessEntityType,
) -> Result<Response, ApiError> {
    require_business_mutation(&principal)?;
    let expected = parse_if_match(&headers)?;
    let idempotency = match begin_item_idempotency(
        &state,
        entity_type,
        "status",
        id,
        expected,
        &request,
        &principal,
        &headers,
    )
    .await?
    {
        ItemIdempotency::Replay(response) => return Ok(response),
        ItemIdempotency::Fresh(context) => context,
    };
    let before = business_inbox::get(&state, entity_type, id).await?;
    let item = business_inbox::update_status(
        &state,
        entity_type,
        id,
        expected,
        request.status,
        &request.reason,
        &principal.email,
    )
    .await?;
    audit_business_mutation(
        &state,
        &headers,
        &principal,
        "business.status",
        entity_type,
        id,
        &before,
        &item,
        &request.reason,
    )
    .await?;
    idempotency
        .complete(&state, &item, StatusCode::OK)
        .await?;
    Ok(entity_response(StatusCode::OK, &item, item.revision))
}

async fn add_rfq_note(
    state: State<AppState>,
    path: Path<Uuid>,
    principal: Extension<AdminPrincipal>,
    headers: HeaderMap,
    request: Json<CreateBusinessNoteRequest>,
) -> Result<Response, ApiError> {
    add_business_note(
        state,
        path,
        principal,
        headers,
        request,
        BusinessEntityType::Rfq,
    )
    .await
}

async fn add_contact_note(
    state: State<AppState>,
    path: Path<Uuid>,
    principal: Extension<AdminPrincipal>,
    headers: HeaderMap,
    request: Json<CreateBusinessNoteRequest>,
) -> Result<Response, ApiError> {
    add_business_note(
        state,
        path,
        principal,
        headers,
        request,
        BusinessEntityType::Contact,
    )
    .await
}

async fn add_business_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<CreateBusinessNoteRequest>,
    entity_type: BusinessEntityType,
) -> Result<Response, ApiError> {
    require_business_mutation(&principal)?;
    let expected = parse_if_match(&headers)?;
    let idempotency = match begin_detail_idempotency(
        &state,
        entity_type,
        id,
        expected,
        &request,
        &principal,
        &headers,
    )
    .await?
    {
        DetailIdempotency::Replay(response) => return Ok(response),
        DetailIdempotency::Fresh(context) => context,
    };
    let detail = business_inbox::add_note(
        &state,
        entity_type,
        id,
        expected,
        &request.body,
        &request.reason,
        principal.user_id,
    )
    .await?;
    audit(
        &state,
        &headers,
        &principal.email,
        "business.note.create",
        entity_type.label(),
        Some(id),
        None,
        None,
        Some(request.reason),
    )
    .await?;
    idempotency
        .complete(&state, &detail, StatusCode::CREATED)
        .await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &detail,
        detail.item.revision,
    ))
}
