use super::*;

pub(super) fn non_negative_finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
}

pub(super) fn positive_finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
}

pub(super) fn valid_fact_state(state: Option<&str>) -> bool {
    matches!(
        state,
        Some(
            "verified"
                | "missing"
                | "notApplicable"
                | "notTested"
                | "confidential"
                | "pendingVerification"
        )
    )
}

/// Canonical, dimension-aware Product Master units. This is intentionally a
/// schema registry, not a source of model values. New units must be introduced
/// through a versioned mapping change rather than accepted as free text.
pub(super) fn valid_specification_unit(unit: &str) -> bool {
    matches!(
        unit,
        "m3/h"
            | "m³/h"
            | "cfm"
            | "L/s"
            | "Pa"
            | "kPa"
            | "inH2O"
            | "mmH2O"
            | "W"
            | "kW"
            | "hp"
            | "V"
            | "kV"
            | "A"
            | "mA"
            | "Hz"
            | "rpm"
            | "mm"
            | "cm"
            | "m"
            | "in"
            | "g"
            | "kg"
            | "lb"
            | "°C"
            | "°F"
            | "K"
            | "dB"
            | "dB(A)"
            | "%"
            | "N·m"
            | "kg/m3"
            | "kg/m³"
    )
}

pub(super) fn unit_requires_operating_condition(unit: &str) -> bool {
    matches!(
        unit,
        "m3/h"
            | "m³/h"
            | "cfm"
            | "L/s"
            | "Pa"
            | "kPa"
            | "inH2O"
            | "mmH2O"
            | "W"
            | "kW"
            | "hp"
            | "A"
            | "mA"
            | "rpm"
            | "dB"
            | "dB(A)"
            | "%"
    )
}

pub(super) fn require_provenance(
    value: &serde_json::Map<String, Value>,
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    if value
        .get("sourceReference")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        issues.push(ValidationIssue {
            field_path: format!("{path}.sourceReference"),
            code: "provenanceRequired".into(),
            detail: "Verified values require a traceable Product Master source reference.".into(),
        });
    }
}

pub(super) fn required_text(
    record: &serde_json::Map<String, Value>,
    field: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    required_text_at(record, field, "", issues);
}

pub(super) fn required_text_at(
    record: &serde_json::Map<String, Value>,
    field: &str,
    prefix: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    if record
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        issues.push(ValidationIssue {
            field_path: if prefix.is_empty() {
                field.into()
            } else {
                format!("{prefix}.{field}")
            },
            code: "required".into(),
            detail: format!("{field} is required."),
        });
    }
}
