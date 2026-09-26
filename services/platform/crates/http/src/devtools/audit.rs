use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use airtek_domain::models::AuditEvent;
use airtek_runtime::error::ApiError;
use airtek_runtime::state::{AppState, DevtoolTokenGrant};

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
            current_version: None,
            request_id,
            occurred_at: Utc::now(),
        })
        .await
}
