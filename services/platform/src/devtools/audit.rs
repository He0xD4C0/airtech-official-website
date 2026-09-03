use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::AuditEvent,
    state::{AppState, DevtoolTokenGrant},
};

pub(super) async fn audit_command(
    state: &AppState,
    grant: &DevtoolTokenGrant,
    request_id: Uuid,
    sequence: u64,
    byte_count: usize,
    command_count: usize,
    transport: &'static str,
) -> Result<(), ApiError> {
    persist_devtools_audit(
        state,
        grant,
        "devtools.command.submitted",
        Some(json!({
            "sequence": sequence,
            "byteCount": byte_count,
            "commandCount": command_count,
            "transport": transport,
            "commandFrame": true,
        })),
        "Shell command submitted; command content intentionally excluded from the audit record.",
        request_id,
    )
    .await
}

pub(super) async fn persist_devtools_audit(
    state: &AppState,
    grant: &DevtoolTokenGrant,
    action: &str,
    after: Option<serde_json::Value>,
    reason: &str,
    request_id: Uuid,
) -> Result<(), ApiError> {
    state
        .persist_audit(AuditEvent {
            id: Uuid::new_v4(),
            actor: grant.actor.clone(),
            action: action.into(),
            entity_type: "devtoolsSession".into(),
            entity_id: Some(grant.session_id),
            before: None,
            after,
            reason: Some(reason.into()),
            request_id,
            occurred_at: Utc::now(),
        })
        .await
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::devtools::limits::TOKEN_TTL;

    #[tokio::test]
    async fn command_audit_stores_metadata_without_command_text() {
        let state = AppState::for_test();
        let grant = DevtoolTokenGrant {
            expires_at: Instant::now() + TOKEN_TTL,
            session_id: Uuid::new_v4(),
            actor: "developer@example.com".into(),
            user_id: Uuid::new_v4(),
            admin_session_id: Uuid::new_v4(),
        };
        audit_command(&state, &grant, Uuid::new_v4(), 1, 31, 1, "control")
            .await
            .unwrap();
        let events = &state.data.read().await.audit_events;
        let event = events.last().unwrap();
        let serialized = serde_json::to_string(event).unwrap();
        assert_eq!(event.actor, "developer@example.com");
        assert_eq!(event.entity_id, Some(grant.session_id));
        assert_eq!(event.action, "devtools.command.submitted");
        assert_eq!(event.after.as_ref().unwrap()["byteCount"], 31);
        assert!(!serialized.contains("airtekctl"));
        assert!(!serialized.contains("commandText"));
    }
}
