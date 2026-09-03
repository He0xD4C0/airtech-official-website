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
