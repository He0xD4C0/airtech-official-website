use axum::extract::ws::{Message, WebSocket};
use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::limits::{MAX_COLS, MAX_ROWS};

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum ClientEvent {
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
pub(super) enum ServerEvent<'a> {
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

pub(super) fn valid_size(rows: u16, cols: u16) -> bool {
    (2..=MAX_ROWS).contains(&rows) && (2..=MAX_COLS).contains(&cols)
}

pub(super) fn command_count(input: &[u8], command_hint: bool) -> usize {
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

pub(super) async fn send_server_event(
    socket: &mut WebSocket,
    event: &ServerEvent<'_>,
) -> Result<(), axum::Error> {
    let value = serde_json::to_string(event).expect("server terminal events are serializable");
    socket.send(Message::Text(value.into())).await
}

pub(super) async fn send_split_event<S>(
    sender: &mut S,
    event: &ServerEvent<'_>,
) -> Result<(), axum::Error>
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
}
