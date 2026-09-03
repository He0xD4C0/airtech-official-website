//! Shared protocol schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "OpenApiDocument".into(),
        json!({"type": "object", "additionalProperties": true}),
    );
    s.insert("ProblemDetails".into(), json!({
        "type": "object", "additionalProperties": false,
        "required": ["type", "title", "status", "detail", "requestId"],
        "properties": {
            "type": {"type": "string", "format": "uri"}, "title": {"type": "string"},
            "status": {"type": "integer", "minimum": 400, "maximum": 599}, "detail": {"type": "string"},
            "instance": nullable(json!({"type": "string"})), "requestId": uuid(),
            "errors": {"type": "object", "additionalProperties": {"type": "array", "items": {"type": "string"}}}
        }
    }));
    s.insert("HealthStatus".into(), object(
        &["status", "service", "version", "persistence", "timestamp"],
        json!({"status": {"type": "string"}, "service": {"type": "string"}, "version": {"type": "string"}, "persistence": {"type": "string"}, "timestamp": timestamp()})
    ));
}
