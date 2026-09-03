use std::{
    env,
    io::{Read, Write},
    time::Duration,
};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::json;
use tokio::{
    sync::{mpsc, OwnedSemaphorePermit},
    time::Instant as TokioInstant,
};
use uuid::Uuid;

use super::{
    access::current_uid,
    audit::{audit_command, persist_devtools_audit},
    limits::{ABSOLUTE_TIMEOUT, IDLE_TIMEOUT, MAX_INPUT_BYTES, MAX_SESSION_OUTPUT_BYTES},
    protocol::{
        command_count, send_server_event, send_split_event, valid_size, ClientEvent, ServerEvent,
    },
};
use crate::state::{AppState, DevtoolTokenGrant};

#[derive(Debug)]
struct Termination {
    reason: &'static str,
    exit_code: Option<u32>,
    input_frames: u64,
    output_bytes: u64,
}

pub(super) async fn terminal_session(
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
