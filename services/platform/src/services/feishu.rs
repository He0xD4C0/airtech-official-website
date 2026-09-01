use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::models::{FieldDiff, ValidationIssue};

/// Builds the source-aware three-way diff shown by the management portal.
/// A field is returned only when the local working record or incoming Feishu
/// snapshot differs from the last accepted source snapshot.
pub fn three_way_diff(
    last_accepted_base: &Value,
    current_local: &Value,
    incoming_feishu: &Value,
    source_owned_paths: &BTreeSet<String>,
) -> Vec<FieldDiff> {
    let base = flatten(last_accepted_base);
    let local = flatten(current_local);
    let incoming = flatten(incoming_feishu);
    let paths: BTreeSet<_> = base
        .keys()
        .chain(local.keys())
        .chain(incoming.keys())
        .cloned()
        .collect();

    paths
        .into_iter()
        .filter_map(|path| {
            let base_value = base.get(&path).cloned();
            let local_value = local.get(&path).cloned();
            let incoming_value = incoming.get(&path).cloned();
            if base_value == local_value && base_value == incoming_value {
                return None;
            }
            Some(FieldDiff {
                field_path: path.clone(),
                base_value,
                local_value,
                incoming_value,
                source_owned: source_owned_paths.contains(&path),
            })
        })
        .collect()
}

pub fn conflicting_diffs(diffs: &[FieldDiff]) -> Vec<FieldDiff> {
    diffs
        .iter()
        .filter(|diff| {
            diff.source_owned
                && diff.local_value != diff.base_value
                && diff.incoming_value != diff.base_value
                && diff.local_value != diff.incoming_value
        })
        .cloned()
        .collect()
}

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

fn validate_specifications(value: Option<&Value>, issues: &mut Vec<ValidationIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(specifications) = value.as_array() else {
        issues.push(ValidationIssue {
            field_path: "specifications".into(),
            code: "arrayRequired".into(),
            detail: "Specifications must be an array with explicit fact states.".into(),
        });
        return;
    };
    let mut keys = BTreeSet::new();
    for (index, specification) in specifications.iter().enumerate() {
        let path = format!("specifications[{index}]");
        let Some(specification) = specification.as_object() else {
            issues.push(ValidationIssue {
                field_path: path,
                code: "objectRequired".into(),
                detail: "Specification must be an object.".into(),
            });
            continue;
        };
        required_text_at(specification, "key", &path, issues);
        required_text_at(specification, "label", &path, issues);
        if let Some(key) = specification
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
        {
            if !keys.insert(key.to_owned()) {
                issues.push(ValidationIssue {
                    field_path: format!("{path}.key"),
                    code: "duplicateKey".into(),
                    detail: "Specification keys must be unique within a product.".into(),
                });
            }
        }
        let unit = specification
            .get("unit")
            .filter(|unit| !unit.is_null())
            .and_then(Value::as_str);
        if unit.is_some_and(|unit| !valid_specification_unit(unit))
            || specification
                .get("unit")
                .is_some_and(|unit| !unit.is_null() && !unit.is_string())
        {
            issues.push(ValidationIssue {
                field_path: format!("{path}.unit"),
                code: "unsupportedUnit".into(),
                detail: "Specification units must use the normalized Product Master unit registry."
                    .into(),
            });
        }
        let operating_condition = specification
            .get("operatingCondition")
            .filter(|condition| !condition.is_null())
            .and_then(Value::as_str);
        if operating_condition.is_some_and(|condition| {
            condition.trim().is_empty()
                || condition.len() > 500
                || condition.chars().any(char::is_control)
        }) || specification
            .get("operatingCondition")
            .is_some_and(|condition| !condition.is_null() && !condition.is_string())
        {
            issues.push(ValidationIssue {
                field_path: format!("{path}.operatingCondition"),
                code: "invalidOperatingCondition".into(),
                detail: "operatingCondition must be a non-empty printable string of at most 500 characters."
                    .into(),
            });
        }
        let state = specification.get("state").and_then(Value::as_str);
        if !valid_fact_state(state) {
            issues.push(ValidationIssue {
                field_path: format!("{path}.state"),
                code: "factStateRequired".into(),
                detail: "Every specification requires an explicit fact state.".into(),
            });
        }
        if state == Some("verified") {
            if specification.get("value").is_none_or(Value::is_null) {
                issues.push(ValidationIssue {
                    field_path: format!("{path}.value"),
                    code: "verifiedValueRequired".into(),
                    detail: "A verified specification requires a value.".into(),
                });
            }
            require_provenance(specification, &path, issues);
            if unit.is_some_and(unit_requires_operating_condition)
                && operating_condition
                    .map(str::trim)
                    .filter(|condition| !condition.is_empty())
                    .is_none()
            {
                issues.push(ValidationIssue {
                    field_path: format!("{path}.operatingCondition"),
                    code: "operatingConditionRequired".into(),
                    detail: "Performance-dependent verified values require an operating condition."
                        .into(),
                });
            }
        } else if state == Some("pendingVerification") {
            issues.push(ValidationIssue {
                field_path: format!("{path}.state"),
                code: "pendingVerification".into(),
                detail: "Pending Product Master facts cannot be published.".into(),
            });
        } else if state.is_some()
            && specification
                .get("value")
                .is_some_and(|value| !value.is_null())
        {
            issues.push(ValidationIssue {
                field_path: format!("{path}.value"),
                code: "unverifiedValue".into(),
                detail: "Only verified Product Master facts may contain a publishable value."
                    .into(),
            });
        }
    }
}

fn validate_performance_curves(value: Option<&Value>, issues: &mut Vec<ValidationIssue>) {
    let Some(value) = value else {
        return;
    };
    let Some(curves) = value.as_array() else {
        issues.push(ValidationIssue {
            field_path: "performanceCurves".into(),
            code: "arrayRequired".into(),
            detail: "Performance curves must be an array.".into(),
        });
        return;
    };
    for (index, curve) in curves.iter().enumerate() {
        let path = format!("performanceCurves[{index}]");
        let Some(curve) = curve.as_object() else {
            issues.push(ValidationIssue {
                field_path: path,
                code: "objectRequired".into(),
                detail: "A performance curve must be an object.".into(),
            });
            continue;
        };
        validate_curve_unit(
            curve,
            "airflowUnit",
            &["m3/h", "m³/h", "cfm"],
            &path,
            issues,
        );
        validate_curve_unit(
            curve,
            "pressureUnit",
            &["Pa", "kPa", "inH2O"],
            &path,
            issues,
        );
        let state = curve.get("state").and_then(Value::as_str);
        if !valid_fact_state(state) {
            issues.push(ValidationIssue {
                field_path: format!("{path}.state"),
                code: "factStateRequired".into(),
                detail: "Every performance curve requires an explicit fact state.".into(),
            });
        }
        if state == Some("verified") {
            require_provenance(curve, &path, issues);
            validate_curve_points(curve.get("points"), &path, issues);
            if curve
                .get("testMethod")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|method| !method.is_empty() && method.len() <= 300)
                .is_none()
            {
                issues.push(ValidationIssue {
                    field_path: format!("{path}.testMethod"),
                    code: "testMethodRequired".into(),
                    detail: "A verified performance curve requires a traceable test method.".into(),
                });
            }
        } else if state == Some("pendingVerification") {
            issues.push(ValidationIssue {
                field_path: format!("{path}.state"),
                code: "pendingVerification".into(),
                detail: "Pending performance curves cannot be published.".into(),
            });
        } else if state.is_some()
            && curve
                .get("points")
                .and_then(Value::as_array)
                .is_some_and(|points| !points.is_empty())
        {
            issues.push(ValidationIssue {
                field_path: format!("{path}.points"),
                code: "unverifiedCurveData".into(),
                detail: "Only verified performance curves may contain publishable points.".into(),
            });
        }
        validate_positive_number(curve.get("densityKgM3"), "densityKgM3", &path, issues);
        validate_positive_number(curve.get("speedRpm"), "speedRpm", &path, issues);
    }
}

fn validate_curve_unit(
    curve: &serde_json::Map<String, Value>,
    field: &str,
    allowed: &[&str],
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    let unit = curve.get(field).and_then(Value::as_str);
    if unit.is_none_or(|unit| !allowed.contains(&unit)) {
        issues.push(ValidationIssue {
            field_path: format!("{path}.{field}"),
            code: "unsupportedUnit".into(),
            detail: format!(
                "{field} must use one of the normalized units: {}.",
                allowed.join(", ")
            ),
        });
    }
}

fn validate_curve_points(value: Option<&Value>, path: &str, issues: &mut Vec<ValidationIssue>) {
    let Some(points) = value.and_then(Value::as_array) else {
        issues.push(ValidationIssue {
            field_path: format!("{path}.points"),
            code: "curvePointsRequired".into(),
            detail: "A verified curve requires an array of at least two points.".into(),
        });
        return;
    };
    if points.len() < 2 {
        issues.push(ValidationIssue {
            field_path: format!("{path}.points"),
            code: "curvePointsRequired".into(),
            detail: "A verified curve requires at least two points.".into(),
        });
    }
    let mut previous_airflow = None;
    for (index, point) in points.iter().enumerate() {
        let point_path = format!("{path}.points[{index}]");
        let Some(point) = point.as_object() else {
            issues.push(ValidationIssue {
                field_path: point_path,
                code: "objectRequired".into(),
                detail: "A curve point must be an object.".into(),
            });
            continue;
        };
        let airflow = non_negative_finite(point.get("airflow"));
        let pressure = non_negative_finite(point.get("pressure"));
        if airflow.is_none() {
            issues.push(ValidationIssue {
                field_path: format!("{point_path}.airflow"),
                code: "invalidCurvePoint".into(),
                detail: "airflow must be a non-negative finite number.".into(),
            });
        }
        if pressure.is_none() {
            issues.push(ValidationIssue {
                field_path: format!("{point_path}.pressure"),
                code: "invalidCurvePoint".into(),
                detail: "pressure must be a non-negative finite number.".into(),
            });
        }
        if let Some(airflow) = airflow {
            if previous_airflow.is_some_and(|previous| airflow <= previous) {
                issues.push(ValidationIssue {
                    field_path: format!("{point_path}.airflow"),
                    code: "nonIncreasingAirflow".into(),
                    detail: "Verified curve airflow points must be strictly increasing.".into(),
                });
            }
            previous_airflow = Some(airflow);
        }
    }
}

fn validate_positive_number(
    value: Option<&Value>,
    field: &str,
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    if value.is_some_and(|value| !value.is_null() && positive_finite(Some(value)).is_none()) {
        issues.push(ValidationIssue {
            field_path: format!("{path}.{field}"),
            code: "positiveNumberRequired".into(),
            detail: format!("{field} must be a positive finite number when provided."),
        });
    }
}

fn non_negative_finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
}

fn positive_finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn valid_fact_state(state: Option<&str>) -> bool {
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
fn valid_specification_unit(unit: &str) -> bool {
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

fn unit_requires_operating_condition(unit: &str) -> bool {
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

fn require_provenance(
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

fn required_text(
    record: &serde_json::Map<String, Value>,
    field: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    required_text_at(record, field, "", issues);
}

fn required_text_at(
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

fn flatten(value: &Value) -> BTreeMap<String, Value> {
    let mut output = BTreeMap::new();
    flatten_at("", value, &mut output);
    output
}

fn flatten_at(path: &str, value: &Value, output: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                flatten_at(&child_path, child, output);
            }
        }
        _ => {
            output.insert(path.to_owned(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::{conflicting_diffs, three_way_diff, validate_staging_payload};

    #[test]
    fn detects_source_owned_three_way_conflict() {
        let base = json!({"model": "A", "site": {"seoTitle": "Old"}});
        let local = json!({"model": "LOCAL", "site": {"seoTitle": "New"}});
        let incoming = json!({"model": "FEISHU", "site": {"seoTitle": "Old"}});
        let owned = BTreeSet::from(["model".to_owned()]);
        let diffs = three_way_diff(&base, &local, &incoming, &owned);
        let conflicts = conflicting_diffs(&diffs);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field_path, "model");
    }

    #[test]
    fn verified_values_require_product_master_provenance() {
        let issues = validate_staging_payload(&json!({
            "stableId": "source-record-id",
            "sourceRevision": "revision-id",
            "family": "axial",
            "specifications": [{"key": "airflow", "state": "verified", "value": 1}]
        }));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "provenanceRequired"));
    }

    #[test]
    fn validates_verified_pq_units_provenance_and_monotonic_points() {
        let issues = validate_staging_payload(&json!({
            "stableId": "AT_VALIDATED-001",
            "model": "VALIDATED-MODEL",
            "sourceRevision": "revision-id",
            "family": "axial",
            "performanceCurves": [{
                "airflowUnit": "litres/minute",
                "pressureUnit": "Pa",
                "state": "verified",
                "sourceReference": "",
                "densityKgM3": -1,
                "points": [
                    {"airflow": 100, "pressure": 200},
                    {"airflow": 100, "pressure": 180}
                ]
            }]
        }));
        for code in [
            "unsupportedUnit",
            "provenanceRequired",
            "positiveNumberRequired",
            "nonIncreasingAirflow",
        ] {
            assert!(issues.iter().any(|issue| issue.code == code), "{code}");
        }
    }

    #[test]
    fn accepts_a_traceable_normalized_verified_product_payload() {
        let issues = validate_staging_payload(&json!({
            "stableId": "AT_VALIDATED-001",
            "model": "VALIDATED-MODEL",
            "sourceRevision": "revision-id",
            "family": "centrifugal",
            "specifications": [{
                "key": "ratedVoltage",
                "label": "Rated voltage",
                "value": 230,
                "unit": "V",
                "state": "verified",
                "sourceReference": "feishu:record:field"
            }],
            "performanceCurves": [{
                "airflowUnit": "m3/h",
                "pressureUnit": "Pa",
                "state": "verified",
                "sourceReference": "feishu:curve:1",
                "testMethod": "Controlled Product Master test",
                "speedRpm": 1200,
                "densityKgM3": 1.2,
                "points": [
                    {"airflow": 0, "pressure": 300},
                    {"airflow": 100, "pressure": 200}
                ]
            }]
        }));
        assert!(issues.is_empty(), "{issues:#?}");
    }

    #[test]
    fn rejects_duplicate_specification_keys_and_unverified_values_without_state() {
        let issues = validate_staging_payload(&json!({
            "stableId": "AT_VALIDATED-001",
            "sourceRevision": "revision-id",
            "family": "motors",
            "specifications": [
                {"key": "power", "label": "Power", "state": "missing"},
                {"key": "power", "label": "Power", "value": 1}
            ]
        }));
        assert!(issues.iter().any(|issue| issue.code == "duplicateKey"));
        assert!(issues.iter().any(|issue| issue.code == "factStateRequired"));
    }
}
