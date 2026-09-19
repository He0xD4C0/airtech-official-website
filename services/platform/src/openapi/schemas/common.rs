//! Shared protocol schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;
use crate::error::{problem_type_uri, STABLE_DOMAIN_PROBLEM_CODES};

pub(super) fn add(s: &mut Map<String, Value>) {
    let stable_problem_types = STABLE_DOMAIN_PROBLEM_CODES
        .iter()
        .map(|code| problem_type_uri(code))
        .collect::<Vec<_>>();
    s.insert(
        "StableProblemCode".into(),
        json!({"type": "string", "enum": STABLE_DOMAIN_PROBLEM_CODES}),
    );
    s.insert(
        "StableProblemType".into(),
        json!({"type": "string", "format": "uri", "enum": stable_problem_types}),
    );
    s.insert(
        "OpenApiDocument".into(),
        json!({"type": "object", "additionalProperties": true}),
    );
    s.insert("ProblemDetails".into(), json!({
        "type": "object", "additionalProperties": false,
        "required": ["type", "title", "status", "detail", "requestId"],
        "properties": {
            "type": {
                "type": "string",
                "format": "uri",
                "description": "Stable application problem URIs are enumerated by StableProblemType. Numeric fallback URIs remain valid for generic protocol failures.",
                "x-stable-domain-types": stable_problem_types
            },
            "title": {"type": "string"},
            "status": {"type": "integer", "minimum": 400, "maximum": 599}, "detail": {"type": "string"},
            "instance": nullable(json!({"type": "string"})), "requestId": uuid(),
            "activeRunId": nullable(uuid()),
            "errors": {"type": "object", "additionalProperties": {"type": "array", "items": {"type": "string"}}},
            "issues": array(r("DependencyProblemIssue"))
        }
    }));
    s.insert(
        "DependencyProblemIssue".into(),
        object(
            &["code", "path", "failedGate", "targetId", "detail"],
            json!({
                "code": {"type": "string"},
                "path": {"type": "string", "minLength": 1},
                "failedGate": {"type": "string"},
                "targetId": nullable(uuid()),
                "detail": {"type": "string", "minLength": 1}
            }),
        ),
    );
    s.insert("HealthStatus".into(), object(
        &["status", "service", "version", "persistence", "timestamp"],
        json!({"status": {"type": "string"}, "service": {"type": "string"}, "version": {"type": "string"}, "persistence": {"type": "string"}, "timestamp": timestamp()})
    ));
}
