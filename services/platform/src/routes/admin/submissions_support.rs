enum ItemIdempotency {
    Replay(Response),
    Fresh(crate::idempotency::IdempotencyContext),
}

enum DetailIdempotency {
    Replay(Response),
    Fresh(crate::idempotency::IdempotencyContext),
}

#[allow(clippy::too_many_arguments)]
async fn begin_item_idempotency<T: serde::Serialize>(
    state: &AppState,
    entity_type: BusinessEntityType,
    action: &str,
    id: Uuid,
    expected: i64,
    request: &T,
    principal: &AdminPrincipal,
    headers: &HeaderMap,
) -> Result<ItemIdempotency, ApiError> {
    match begin_idempotency(
        state,
        item_operation(entity_type, action),
        headers,
        &json!({"actor": principal.user_id, "id": id, "ifMatch": expected, "request": request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let item: BusinessInboxItem = replay.decode()?;
            Ok(ItemIdempotency::Replay(entity_response(
                status,
                &item,
                item.revision,
            )))
        }
        IdempotencyOutcome::Fresh(context) => Ok(ItemIdempotency::Fresh(context)),
    }
}

async fn begin_detail_idempotency<T: serde::Serialize>(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
    expected: i64,
    request: &T,
    principal: &AdminPrincipal,
    headers: &HeaderMap,
) -> Result<DetailIdempotency, ApiError> {
    match begin_idempotency(
        state,
        note_operation(entity_type),
        headers,
        &json!({"actor": principal.user_id, "id": id, "ifMatch": expected, "request": request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let detail: BusinessInboxDetail = replay.decode()?;
            Ok(DetailIdempotency::Replay(entity_response(
                status,
                &detail,
                detail.item.revision,
            )))
        }
        IdempotencyOutcome::Fresh(context) => Ok(DetailIdempotency::Fresh(context)),
    }
}

fn item_operation(entity_type: BusinessEntityType, action: &str) -> &'static str {
    match (entity_type, action) {
        (BusinessEntityType::Rfq, "assign") => "admin.business.rfq.assign",
        (BusinessEntityType::Rfq, _) => "admin.business.rfq.status",
        (BusinessEntityType::Contact, "assign") => "admin.business.contact.assign",
        (BusinessEntityType::Contact, _) => "admin.business.contact.status",
    }
}

fn note_operation(entity_type: BusinessEntityType) -> &'static str {
    match entity_type {
        BusinessEntityType::Rfq => "admin.business.rfq.note",
        BusinessEntityType::Contact => "admin.business.contact.note",
    }
}

fn parse_business_status(value: String) -> Result<BusinessInboxStatus, ApiError> {
    serde_json::from_value(Value::String(value)).map_err(|_| {
        ApiError::bad_request("status is not a controlled business workflow state.")
    })
}

fn require_business_mutation(principal: &AdminPrincipal) -> Result<(), ApiError> {
    if principal.has_permission("rfq.assign") {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "The `rfq.assign` permission is required.",
        ))
    }
}

#[allow(clippy::too_many_arguments)]
async fn audit_business_mutation(
    state: &AppState,
    headers: &HeaderMap,
    principal: &AdminPrincipal,
    action: &str,
    entity_type: BusinessEntityType,
    id: Uuid,
    before: &BusinessInboxItem,
    after: &BusinessInboxItem,
    reason: &str,
) -> Result<(), ApiError> {
    audit(
        state,
        headers,
        &principal.email,
        action,
        entity_type.label(),
        Some(id),
        Some(json!(before)),
        Some(json!(after)),
        Some(reason.into()),
    )
    .await
}
