use super::*;

/// Performs transport-independent validation before a Feishu record may leave staging.
/// It intentionally validates provenance and structure, not product-specific numeric ranges.
pub fn validate_staging_payload(payload: &Value) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let Some(record) = payload.as_object() else {
        return vec![ValidationIssue {
            field_path: "$".into(),
            code: "objectRequired".into(),
            detail: "A Feishu source record must be a JSON object.".into(),
        }];
    };
    required_text(record, "stableId", &mut issues);
    if let Some(stable_id) = record.get("stableId").and_then(Value::as_str) {
        let stable_id = stable_id.trim();
        if stable_id.len() > 200
            || stable_id.is_empty()
            || !stable_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            issues.push(ValidationIssue {
                field_path: "stableId".into(),
                code: "invalidStableId".into(),
                detail:
                    "stableId must be 1 to 200 ASCII letters, digits, dots, underscores or hyphens."
                        .into(),
            });
        }
    }
    required_text(record, "model", &mut issues);
    if record
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(|model| model.trim().len() > 200 || model.chars().any(char::is_control))
    {
        issues.push(ValidationIssue {
            field_path: "model".into(),
            code: "invalidModel".into(),
            detail: "model must contain at most 200 printable characters.".into(),
        });
    }
    required_text(record, "sourceRevision", &mut issues);
    if let Some(family) = record.get("family").and_then(Value::as_str) {
        if !["centrifugal", "axial", "crossFlow", "inlineDuct", "motors"].contains(&family) {
            issues.push(ValidationIssue {
                field_path: "family".into(),
                code: "unsupportedFamily".into(),
                detail: "Family must use the normalized AIRTEKPOWER taxonomy.".into(),
            });
        }
    } else {
        issues.push(ValidationIssue {
            field_path: "family".into(),
            code: "required".into(),
            detail: "A normalized product family is required.".into(),
        });
    }
    validate_specifications(record.get("specifications"), &mut issues);
    validate_performance_curves(record.get("performanceCurves"), &mut issues);
    issues
}
