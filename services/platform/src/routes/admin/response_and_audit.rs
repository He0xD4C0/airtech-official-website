use super::*;

pub(super) fn add_private_no_store_headers(response: &mut Response) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}

// Keeping these fields explicit at the mutation call sites makes every audit
// record reviewable there (actor, request, entity, before/after and reason)
// instead of hiding security-relevant values behind a partially filled map.
#[allow(clippy::too_many_arguments)]
pub(super) async fn audit(
    state: &AppState,
    headers: &HeaderMap,
    actor: &str,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> Result<(), ApiError> {
    state
        .persist_audit(AuditEvent {
            id: Uuid::new_v4(),
            actor: actor.into(),
            action: action.into(),
            entity_type: entity_type.into(),
            entity_id,
            before,
            after,
            reason,
            current_version: None,
            request_id: headers
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| Uuid::parse_str(value).ok())
                .unwrap_or_else(Uuid::new_v4),
            occurred_at: Utc::now(),
        })
        .await
}
