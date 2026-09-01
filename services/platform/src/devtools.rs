use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    process::Command,
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Extension, Query, State,
    },
    http::HeaderMap,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::{
    sync::{mpsc, OwnedSemaphorePermit},
    time::Instant as TokioInstant,
};
use uuid::Uuid;

use crate::{
    auth::AdminPrincipal,
    error::ApiError,
    models::AuditEvent,
    state::{AppState, DevtoolTokenGrant},
};

const TOKEN_TTL: Duration = Duration::from_secs(60);
const MAX_PENDING_TOKENS: usize = 32;
const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const ABSOLUTE_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_SESSION_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ROWS: u16 = 240;
const MAX_COLS: u16 = 500;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalToken {
    token: String,
    expires_in_seconds: u64,
}

#[derive(Deserialize)]
struct TerminalQuery {
    token: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ClientEvent {
    Input {
        data: String,
        #[serde(default)]
        command: bool,
    },
    Resize {
        rows: u16,
        cols: u16,
        #[serde(default)]
        pixel_width: u16,
        #[serde(default)]
        pixel_height: u16,
    },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ServerEvent<'a> {
    SessionStarted {
        session_id: Uuid,
        idle_timeout_seconds: u64,
        absolute_timeout_seconds: u64,
        max_output_bytes: u64,
    },
    SessionEnded {
        session_id: Uuid,
        reason: &'a str,
        exit_code: Option<u32>,
    },
    Error {
        code: &'a str,
        message: &'a str,
    },
}

#[derive(Debug)]
struct Termination {
    reason: &'static str,
    exit_code: Option<u32>,
    input_frames: u64,
    output_bytes: u64,
}

pub fn protected_router() -> Router<AppState> {
    Router::new().route("/sessions/token", post(create_terminal_token))
}

pub fn terminal_router() -> Router<AppState> {
    Router::new().route("/terminal", get(open_terminal))
}

pub fn ensure_non_root() -> Result<(), ApiError> {
    #[cfg(unix)]
    {
        let uid = current_uid()?;
        if uid == "0" {
            return Err(ApiError::new(
                axum::http::StatusCode::FORBIDDEN,
                "Root is not allowed",
                "Development PTY sessions must run as a non-root operating-system user.",
            ));
        }
    }
    Ok(())
}

fn current_uid() -> Result<String, ApiError> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|_| ApiError::internal("Unable to verify the operating-system user."))?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || uid.is_empty() {
        return Err(ApiError::internal(
            "Unable to verify the operating-system user.",
        ));
    }
    Ok(uid)
}

async fn create_terminal_token(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
) -> Result<Json<TerminalToken>, ApiError> {
    ensure_non_root()?;
    let uid = current_uid()?;
    reject_non_admin_origin(&state, &headers)?;
    if state.devtool_session_slots.available_permits() == 0 {
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

    {
        let mut tokens = state.devtool_tokens.write().await;
        let now = Instant::now();
        tokens.retain(|_, value| value.expires_at > now);
        if tokens.len() >= MAX_PENDING_TOKENS {
            return Err(ApiError::too_many_requests(
                "Too many unused development terminal authorizations are pending.",
            ));
        }
        tokens.insert(token_hash.clone(), grant.clone());
    }

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
        state.devtool_tokens.write().await.remove(&token_hash);
        return Err(error);
    }

    Ok(Json(TerminalToken {
        token: raw_token,
        expires_in_seconds: TOKEN_TTL.as_secs(),
    }))
}

async fn open_terminal(
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
    let principal = crate::auth::authenticate(&state, &headers).await?;

    let token_hash = terminal_token_hash(&query.token);
    let grant = {
        let mut tokens = state.devtool_tokens.write().await;
        take_terminal_grant(&mut tokens, &token_hash, Instant::now())
    }
    .ok_or_else(|| {
        ApiError::unauthorized("The terminal token is missing, expired, or already used.")
    })?;
    if principal.user_id != grant.user_id || principal.session_id != grant.admin_session_id {
        return Err(ApiError::unauthorized(
            "The terminal authorization does not belong to the active Admin session.",
        ));
    }
    let permit = state
        .devtool_session_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::too_many_requests("The development terminal session limit has been reached.")
        })?;
    let request_id = request_id(&headers);

    Ok(websocket
        .on_upgrade(move |socket| terminal_session(socket, state, grant, request_id, permit)))
}

async fn terminal_session(
    mut socket: WebSocket,
    state: AppState,
    grant: DevtoolTokenGrant,
    request_id: Uuid,
    _permit: OwnedSemaphorePermit,
) {
    let uid = match current_uid() {
        Ok(uid) => uid,
        Err(_) => {
            let _ = send_server_event(
                &mut socket,
                &ServerEvent::Error {
                    code: "uidVerificationFailed",
                    message: "The service operating-system user could not be verified.",
                },
            )
            .await;
            return;
        }
    };

    if let Err(error) = persist_devtools_audit(
        &state,
        &grant,
        "devtools.session.started",
        Some(json!({
            "adminSessionId": grant.admin_session_id,
            "adminUserId": grant.user_id,
            "osUid": uid,
            "idleTimeoutSeconds": IDLE_TIMEOUT.as_secs(),
            "absoluteTimeoutSeconds": ABSOLUTE_TIMEOUT.as_secs(),
            "maxOutputBytes": MAX_SESSION_OUTPUT_BYTES,
        })),
        "Authenticated development PTY session established; no shell input was recorded.",
        request_id,
    )
    .await
    {
        tracing::error!(session_id = %grant.session_id, error = %error, "PTY start audit failed");
        let _ = send_server_event(
            &mut socket,
            &ServerEvent::Error {
                code: "auditUnavailable",
                message:
                    "The terminal was not started because its audit record could not be stored.",
            },
        )
        .await;
        return;
    }

    if send_server_event(
        &mut socket,
        &ServerEvent::SessionStarted {
            session_id: grant.session_id,
            idle_timeout_seconds: IDLE_TIMEOUT.as_secs(),
            absolute_timeout_seconds: ABSOLUTE_TIMEOUT.as_secs(),
            max_output_bytes: MAX_SESSION_OUTPUT_BYTES,
        },
    )
    .await
    .is_err()
    {
        let _ = persist_devtools_audit(
            &state,
            &grant,
            "devtools.session.ended",
            Some(json!({
                "exitCode": null,
                "inputFrames": 0,
                "outputBytes": 0,
            })),
            "clientDisconnectedBeforeReady",
            request_id,
        )
        .await;
        return;
    }

    let result = run_terminal(socket, &state, &grant, request_id).await;
    if let Err(error) = persist_devtools_audit(
        &state,
        &grant,
        "devtools.session.ended",
        Some(json!({
            "exitCode": result.exit_code,
            "inputFrames": result.input_frames,
            "outputBytes": result.output_bytes,
        })),
        result.reason,
        request_id,
    )
    .await
    {
        tracing::error!(session_id = %grant.session_id, error = %error, "PTY end audit failed");
    }
}

async fn run_terminal(
    mut socket: WebSocket,
    state: &AppState,
    grant: &DevtoolTokenGrant,
    request_id: Uuid,
) -> Termination {
    let pair = match native_pty_system().openpty(PtySize {
        rows: 32,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(_) => {
            return terminal_start_error(
                &mut socket,
                grant.session_id,
                "ptyUnavailable",
                "The development PTY could not be created.",
            )
            .await;
        }
    };
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut command = CommandBuilder::new(shell);
    if let Ok(directory) = env::current_dir() {
        command.cwd(directory);
    }
    command.env("TERM", "xterm-256color");
    command.env("AIRTEK_DEVTOOLS_SESSION_ID", grant.session_id.to_string());
    let mut child = match pair.slave.spawn_command(command) {
        Ok(child) => child,
        Err(_) => {
            return terminal_start_error(
                &mut socket,
                grant.session_id,
                "shellUnavailable",
                "The inherited non-root shell could not be started.",
            )
            .await;
        }
    };
    drop(pair.slave);
    let mut reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(_) => {
            let _ = child.kill();
            return terminal_start_error(
                &mut socket,
                grant.session_id,
                "ptyIoUnavailable",
                "The PTY output stream could not be opened.",
            )
            .await;
        }
    };
    let mut writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(_) => {
            let _ = child.kill();
            return terminal_start_error(
                &mut socket,
                grant.session_id,
                "ptyIoUnavailable",
                "The PTY input stream could not be opened.",
            )
            .await;
        }
    };

    let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(32);
    std::thread::spawn(move || {
        let mut buffer = vec![0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if output_tx.blocking_send(buffer[..count].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let (mut sender, mut receiver) = socket.split();
    let absolute_timeout = tokio::time::sleep(ABSOLUTE_TIMEOUT);
    let idle_timeout = tokio::time::sleep(IDLE_TIMEOUT);
    tokio::pin!(absolute_timeout);
    tokio::pin!(idle_timeout);
    let mut child_poll = tokio::time::interval(Duration::from_millis(200));
    child_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut output_open = true;
    let mut input_frames = 0_u64;
    let mut output_bytes = 0_u64;

    let mut termination = loop {
        tokio::select! {
            output = output_rx.recv(), if output_open => {
                match output {
                    Some(output) => {
                        output_bytes = output_bytes.saturating_add(output.len() as u64);
                        if output_bytes > MAX_SESSION_OUTPUT_BYTES {
                            break Termination { reason: "outputLimit", exit_code: None, input_frames, output_bytes };
                        }
                        idle_timeout.as_mut().reset(TokioInstant::now() + IDLE_TIMEOUT);
                        if sender.send(Message::Binary(output.into())).await.is_err() {
                            break Termination { reason: "clientDisconnected", exit_code: None, input_frames, output_bytes };
                        }
                    }
                    None => output_open = false,
                }
            }
            message = receiver.next() => {
                idle_timeout.as_mut().reset(TokioInstant::now() + IDLE_TIMEOUT);
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let event = if text.len() <= MAX_INPUT_BYTES {
                            serde_json::from_str::<ClientEvent>(&text).ok()
                        } else {
                            None
                        };
                        match event {
                            Some(ClientEvent::Input { data, command }) if data.len() <= MAX_INPUT_BYTES => {
                                input_frames = input_frames.saturating_add(1);
                                let command_count = command_count(data.as_bytes(), command);
                                if command_count > 0
                                    && audit_command(state, grant, request_id, input_frames, data.len(), command_count, "control").await.is_err()
                                {
                                    break Termination { reason: "auditUnavailable", exit_code: None, input_frames, output_bytes };
                                }
                                if writer.write_all(data.as_bytes()).and_then(|_| writer.flush()).is_err() {
                                    break Termination { reason: "ptyInputError", exit_code: None, input_frames, output_bytes };
                                }
                            }
                            Some(ClientEvent::Resize { rows, cols, pixel_width, pixel_height }) if valid_size(rows, cols) => {
                                if pair.master.resize(PtySize { rows, cols, pixel_width, pixel_height }).is_err() {
                                    break Termination { reason: "ptyResizeError", exit_code: None, input_frames, output_bytes };
                                }
                            }
                            _ => {
                                break Termination { reason: "invalidControlMessage", exit_code: None, input_frames, output_bytes };
                            }
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) if bytes.len() <= MAX_INPUT_BYTES => {
                        input_frames = input_frames.saturating_add(1);
                        let command_count = command_count(&bytes, false);
                        if command_count > 0
                            && audit_command(state, grant, request_id, input_frames, bytes.len(), command_count, "binary").await.is_err()
                        {
                            break Termination { reason: "auditUnavailable", exit_code: None, input_frames, output_bytes };
                        }
                        if writer.write_all(&bytes).and_then(|_| writer.flush()).is_err() {
                            break Termination { reason: "ptyInputError", exit_code: None, input_frames, output_bytes };
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        break Termination { reason: "inputFrameTooLarge", exit_code: None, input_frames, output_bytes };
                    }
                    Some(Ok(Message::Ping(bytes))) => {
                        if sender.send(Message::Pong(bytes)).await.is_err() {
                            break Termination { reason: "clientDisconnected", exit_code: None, input_frames, output_bytes };
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) => {
                        break Termination { reason: "clientClosed", exit_code: None, input_frames, output_bytes };
                    }
                    Some(Err(_)) | None => {
                        break Termination { reason: "clientDisconnected", exit_code: None, input_frames, output_bytes };
                    }
                }
            }
            _ = child_poll.tick() => {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        break Termination { reason: "shellExited", exit_code: Some(status.exit_code()), input_frames, output_bytes };
                    }
                    Ok(None) => {}
                    Err(_) => {
                        break Termination { reason: "shellStatusError", exit_code: None, input_frames, output_bytes };
                    }
                }
            }
            _ = &mut idle_timeout => {
                break Termination { reason: "idleTimeout", exit_code: None, input_frames, output_bytes };
            }
            _ = &mut absolute_timeout => {
                break Termination { reason: "absoluteTimeout", exit_code: None, input_frames, output_bytes };
            }
        }
    };

    if termination.exit_code.is_none() {
        let _ = child.kill();
        if let Ok(status) = child.wait() {
            termination.exit_code = Some(status.exit_code());
        }
    }
    while let Ok(output) = output_rx.try_recv() {
        let next_output_bytes = termination.output_bytes.saturating_add(output.len() as u64);
        if next_output_bytes > MAX_SESSION_OUTPUT_BYTES {
            termination.reason = "outputLimit";
            break;
        }
        termination.output_bytes = next_output_bytes;
        if sender.send(Message::Binary(output.into())).await.is_err() {
            break;
        }
    }
    let _ = send_split_event(
        &mut sender,
        &ServerEvent::SessionEnded {
            session_id: grant.session_id,
            reason: termination.reason,
            exit_code: termination.exit_code,
        },
    )
    .await;
    let _ = sender
        .send(Message::Close(Some(CloseFrame {
            code: 1000,
            reason: "development PTY ended".into(),
        })))
        .await;
    termination
}

async fn terminal_start_error(
    socket: &mut WebSocket,
    session_id: Uuid,
    code: &'static str,
    message: &'static str,
) -> Termination {
    let _ = send_server_event(socket, &ServerEvent::Error { code, message }).await;
    let _ = send_server_event(
        socket,
        &ServerEvent::SessionEnded {
            session_id,
            reason: code,
            exit_code: None,
        },
    )
    .await;
    Termination {
        reason: code,
        exit_code: None,
        input_frames: 0,
        output_bytes: 0,
    }
}

async fn audit_command(
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

async fn persist_devtools_audit(
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

fn request_id(headers: &HeaderMap) -> Uuid {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

fn reject_non_admin_origin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        if origin != state.config.admin_origin {
            return Err(ApiError::forbidden(
                "Development terminal authorization is accepted only from the configured Admin Web origin.",
            ));
        }
    }
    Ok(())
}

fn terminal_token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn take_terminal_grant(
    tokens: &mut HashMap<String, DevtoolTokenGrant>,
    token_hash: &str,
    now: Instant,
) -> Option<DevtoolTokenGrant> {
    tokens
        .remove(token_hash)
        .filter(|value| value.expires_at > now)
}

fn valid_size(rows: u16, cols: u16) -> bool {
    (2..=MAX_ROWS).contains(&rows) && (2..=MAX_COLS).contains(&cols)
}

fn command_count(input: &[u8], command_hint: bool) -> usize {
    let mut line_endings = 0;
    let mut previous_was_carriage_return = false;
    for byte in input {
        match byte {
            b'\r' => {
                line_endings += 1;
                previous_was_carriage_return = true;
            }
            b'\n' if !previous_was_carriage_return => line_endings += 1,
            _ => previous_was_carriage_return = false,
        }
    }
    line_endings.max(usize::from(command_hint))
}

async fn send_server_event(
    socket: &mut WebSocket,
    event: &ServerEvent<'_>,
) -> Result<(), axum::Error> {
    let value = serde_json::to_string(event).expect("server terminal events are serializable");
    socket.send(Message::Text(value.into())).await
}

async fn send_split_event<S>(sender: &mut S, event: &ServerEvent<'_>) -> Result<(), axum::Error>
where
    S: futures_util::Sink<Message, Error = axum::Error> + Unpin,
{
    let value = serde_json::to_string(event).expect("server terminal events are serializable");
    sender.send(Message::Text(value.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_protocol_distinguishes_commands_and_resize() {
        assert_eq!(
            serde_json::from_str::<ClientEvent>(
                r#"{"type":"input","data":"airtekctl diagnose\n","command":true}"#,
            )
            .unwrap(),
            ClientEvent::Input {
                data: "airtekctl diagnose\n".into(),
                command: true,
            }
        );
        assert_eq!(
            serde_json::from_str::<ClientEvent>(r#"{"type":"resize","rows":40,"cols":140}"#)
                .unwrap(),
            ClientEvent::Resize {
                rows: 40,
                cols: 140,
                pixel_width: 0,
                pixel_height: 0,
            }
        );
    }

    #[test]
    fn terminal_dimensions_are_bounded() {
        assert!(valid_size(2, 2));
        assert!(valid_size(MAX_ROWS, MAX_COLS));
        assert!(!valid_size(1, 80));
        assert!(!valid_size(24, MAX_COLS + 1));
    }

    #[test]
    fn raw_tokens_are_not_used_as_registry_keys() {
        let token = "153b08e3e82d4418b54a96906012b451";
        let digest = terminal_token_hash(token);
        assert_ne!(digest, token);
        assert_eq!(digest.len(), 64);
        assert_eq!(digest, terminal_token_hash(token));
    }

    #[test]
    fn grants_are_one_time_and_expired_grants_fail_closed() {
        let now = Instant::now();
        let grant = DevtoolTokenGrant {
            expires_at: now + TOKEN_TTL,
            session_id: Uuid::new_v4(),
            actor: "developer@example.com".into(),
            user_id: Uuid::new_v4(),
            admin_session_id: Uuid::new_v4(),
        };
        let mut tokens = HashMap::from([("valid".into(), grant.clone())]);
        assert_eq!(
            take_terminal_grant(&mut tokens, "valid", now)
                .unwrap()
                .session_id,
            grant.session_id
        );
        assert!(take_terminal_grant(&mut tokens, "valid", now).is_none());

        tokens.insert(
            "expired".into(),
            DevtoolTokenGrant {
                expires_at: now - Duration::from_secs(1),
                ..grant
            },
        );
        assert!(take_terminal_grant(&mut tokens, "expired", now).is_none());
        assert!(!tokens.contains_key("expired"));
    }

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

    #[test]
    fn command_detection_does_not_require_trusting_the_browser_hint() {
        assert_eq!(command_count(b"echo ready\r", false), 1);
        assert_eq!(command_count(b"one\ntwo\n", false), 2);
        assert_eq!(command_count(b"one\r\ntwo\r\n", false), 2);
        assert_eq!(command_count(b"partial", true), 1);
        assert_eq!(command_count(b"partial", false), 0);
    }

    #[test]
    fn end_event_exposes_status_but_no_terminal_input() {
        let event = serde_json::to_value(ServerEvent::SessionEnded {
            session_id: Uuid::nil(),
            reason: "shellExited",
            exit_code: Some(7),
        })
        .unwrap();
        assert_eq!(event["type"], "sessionEnded");
        assert_eq!(event["exitCode"], 7);
        assert!(event.get("data").is_none());
        assert!(event.get("command").is_none());
    }

    #[test]
    fn configured_limits_are_finite_and_ordered() {
        let token_ttl = std::hint::black_box(TOKEN_TTL);
        let idle_timeout = std::hint::black_box(IDLE_TIMEOUT);
        let absolute_timeout = std::hint::black_box(ABSOLUTE_TIMEOUT);
        let max_pending_tokens = std::hint::black_box(MAX_PENDING_TOKENS);
        let max_input_bytes = std::hint::black_box(MAX_INPUT_BYTES);
        let max_output_bytes = std::hint::black_box(MAX_SESSION_OUTPUT_BYTES);
        assert!(token_ttl < idle_timeout);
        assert!(idle_timeout < absolute_timeout);
        assert!(max_pending_tokens > 4);
        assert!(max_input_bytes <= 64 * 1024);
        assert!(max_output_bytes <= 64 * 1024 * 1024);
    }
}
