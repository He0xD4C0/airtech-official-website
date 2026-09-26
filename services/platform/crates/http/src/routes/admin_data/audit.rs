use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn audit_mutation(
    state: &AppState,
    headers: &HeaderMap,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> Result<(), ApiError> {
    state
        .persist_audit(mutation_audit_event(
            headers,
            action,
            entity_type,
            entity_id,
            before,
            after,
            reason,
        ))
        .await
}

pub(super) fn mutation_audit_event(
    headers: &HeaderMap,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> AuditEvent {
    AuditEvent {
        id: Uuid::new_v4(),
        actor: actor(headers),
        action: action.into(),
        entity_type: entity_type.into(),
        entity_id,
        before,
        after,
        reason,
        current_version: None,
        request_id: request_id(headers),
        occurred_at: Utc::now(),
    }
}
