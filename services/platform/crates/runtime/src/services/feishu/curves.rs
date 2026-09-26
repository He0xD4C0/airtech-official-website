use super::*;

pub(super) fn validate_performance_curves(
    value: Option<&Value>,
    issues: &mut Vec<ValidationIssue>,
) {
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

pub(super) fn validate_curve_unit(
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

pub(super) fn validate_curve_points(
    value: Option<&Value>,
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
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

pub(super) fn validate_positive_number(
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
