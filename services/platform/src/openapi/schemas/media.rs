//! Media review request schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(schemas: &mut Map<String, Value>) {
    schemas.insert(
        "MediaAssetReviewRequest".into(),
        object(
            &["status", "reason"],
            json!({
                "status": string_enum(&["clean", "quarantined"]),
                "reason": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 500,
                    "pattern": r"\S",
                    "description": "Required for every human clean or quarantine decision; surrounding whitespace is trimmed and the reason is recorded in the audit trail."
                }
            }),
        ),
    );
}
