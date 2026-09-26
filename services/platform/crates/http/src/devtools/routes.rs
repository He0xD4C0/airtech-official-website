use std::time::Instant;

use axum::{
    extract::{Extension, Query, State, WebSocketUpgrade},
    http::HeaderMap,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::{
    access::{current_uid, ensure_non_root, reject_non_admin_origin, request_id},
    audit::persist_devtools_audit,
    limits::{MAX_PENDING_TOKENS, TOKEN_TTL},
    session::terminal_session,
    token::terminal_token_hash,
};
use airtek_runtime::auth::AdminPrincipal;
use airtek_runtime::error::ApiError;
use airtek_runtime::state::{AppState, DevtoolTokenGrant};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TerminalToken {
    token: String,
    expires_in_seconds: u64,
}

#[derive(Deserialize)]
pub(super) struct TerminalQuery {
    token: String,
}

pub(super) async fn create_terminal_token(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
) -> Result<Json<TerminalToken>, ApiError> {
    ensure_non_root()?;
    let uid = current_uid()?;
    reject_non_admin_origin(&state, &headers)?;
    if !state.has_devtool_session_capacity() {
        return Err(ApiError::too_many_requests(
            "The development terminal session limit has been reached.",
        ));
    }

    let raw_token = Uuid::new_v4().simple().to_string();
    let token_hash = terminal_token_hash(&raw_token);
    let session_id = Uuid::new_v4();
    let grant = DevtoolTokenGrant {
        expires_at: Instant::now() + TOKEN_TTL,
        session_id,
        actor: principal.email.clone(),
        user_id: principal.user_id,
        admin_session_id: principal.session_id,
    };

    state
        .issue_devtool_grant(token_hash.clone(), grant.clone(), MAX_PENDING_TOKENS)
        .await?;

    if let Err(error) = persist_devtools_audit(
        &state,
        &grant,
        "devtools.authorization.created",
        Some(json!({
            "expiresInSeconds": TOKEN_TTL.as_secs(),
            "osUid": uid,
        })),
        "One-time development PTY authorization issued; no shell input was recorded.",
        request_id(&headers),
    )
    .await
    {
        state.revoke_devtool_grant(&token_hash).await;
        return Err(error);
    }

    Ok(Json(TerminalToken {
        token: raw_token,
        expires_in_seconds: TOKEN_TTL.as_secs(),
    }))
}

pub(super) async fn open_terminal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TerminalQuery>,
    websocket: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    ensure_non_root()?;
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::forbidden("The Admin Web origin is required."))?;
    if origin != state.config.admin_origin {
        return Err(ApiError::forbidden(
            "Development terminal upgrades are accepted only from the configured Admin Web origin.",
        ));
    }
    if query.token.len() != 32 || !query.token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::unauthorized(
            "The terminal token is missing, expired, or already used.",
        ));
    }
    let principal = airtek_runtime::auth::authenticate(&state, &headers).await?;

    let token_hash = terminal_token_hash(&query.token);
    let grant = state.take_devtool_grant(&token_hash).await.ok_or_else(|| {
        ApiError::unauthorized("The terminal token is missing, expired, or already used.")
    })?;
    if principal.user_id != grant.user_id || principal.session_id != grant.admin_session_id {
        return Err(ApiError::unauthorized(
            "The terminal authorization does not belong to the active Admin session.",
        ));
    }
    let permit = state.acquire_devtool_session()?;
    let request_id = request_id(&headers);

    Ok(websocket
        .on_upgrade(move |socket| terminal_session(socket, state, grant, request_id, permit)))
}
