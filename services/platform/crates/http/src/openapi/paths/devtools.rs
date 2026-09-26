//! Development-only API path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/devtools/v1/sessions/token",
        "post",
        admin(
            op(
                "createDevtoolsTerminalToken",
                "Create a one-time PTY token",
                "devtools",
                [(
                    "200",
                    json_response("One-time PTY token", r("TerminalToken")),
                )],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/devtools/v1/terminal",
        "get",
        params(
            op(
                "openDevtoolsTerminal",
                "Upgrade a one-time token to a development-only PTY WebSocket",
                "devtools",
                [("101", empty_response("WebSocket protocol switch"))],
            ),
            vec![query_param(
                "token",
                true,
                json!({"type": "string", "minLength": 1}),
            )],
        ),
    );
}
