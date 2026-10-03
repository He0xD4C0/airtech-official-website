use std::collections::BTreeMap;

use serde_json::Value;

use airtek_domain::models::{Product, PublicationStatus, ValidationIssue};

use super::feishu::validate_staging_payload;

/// Validates the complete working Product record at the publish boundary.
///
/// Staging validation is deliberately repeated here: a stored `valid` flag is
/// evidence about the accepted source payload, but it must never become a way
/// to bypass the current validator after mappings or validation rules change.
pub fn validate_product_master(product: &Product) -> Vec<ValidationIssue> {
    let mut issues = validate_product_core(product);

    if product.source_snapshot_id.is_nil() {
        issues.push(issue(
            "sourceSnapshotId",
            "sourceSnapshotRequired",
            "A traceable Feishu source snapshot is required.",
        ));
    }
    deduplicate(issues)
}

/// Verified CSV products are traceable through their import run and normalized
/// record rather than a Feishu source snapshot.
pub fn validate_verified_csv_product_master(product: &Product) -> Vec<ValidationIssue> {
    deduplicate(validate_product_core(product))
}

fn validate_product_core(product: &Product) -> Vec<ValidationIssue> {
    let mut issues = match serde_json::to_value(product) {
        Ok(payload) => validate_staging_payload(&payload),
        Err(_) => vec![issue(
            "$",
            "invalidProductPayload",
            "The Product Master contains a value that cannot be represented as JSON.",
        )],
    };

    if product.current_revision < 1 {
        issues.push(issue(
            "currentRevision",
            "invalidRevision",
            "The working Product revision must be at least 1.",
        ));
    }
    if product.status == PublicationStatus::Archived {
        issues.push(issue(
            "status",
            "archivedProduct",
            "An archived or deletion-candidate Product cannot be published.",
        ));
    }
    validate_route_text("slug", &product.slug, 180, is_slug, &mut issues);
    validate_route_text("locale", &product.locale, 35, is_locale, &mut issues);
    validate_text("title", &product.title, 300, &mut issues);

    issues
}

/// Revalidates the exact normalized source record referenced by a Product and
/// verifies the source-owned identity that binds the working record to it.
pub fn validate_accepted_staging_payload(
    product: &Product,
    normalized_payload: &Value,
) -> Vec<ValidationIssue> {
    let mut issues = validate_staging_payload(normalized_payload);
    let expected_family = serde_json::to_value(product.family)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned));
    for (field, expected) in [
        ("stableId", Some(product.stable_id.as_str())),
        ("sourceRevision", Some(product.source_revision.as_str())),
        ("model", product.model.as_deref()),
        ("family", expected_family.as_deref()),
    ] {
        let actual = normalized_payload.get(field).and_then(Value::as_str);
        if actual != expected {
            issues.push(issue(
                field,
                "stagingIdentityMismatch",
                "The accepted staging record does not match the working Product source identity.",
            ));
        }
    }
    deduplicate(issues)
}

/// Ensures source-owned fields still equal the accepted normalized snapshot.
/// A currently active temporary override may cover a changed field. The
/// current v1 override grammar is dot-delimited; an override beneath an array
/// root (for example `specifications.voltage`) covers that source-owned root.
pub fn validate_source_owned_alignment(
    product: &Product,
    normalized_payload: &Value,
    active_override_paths: &[String],
) -> Vec<ValidationIssue> {
    let Ok(working_payload) = serde_json::to_value(product) else {
        return vec![issue(
            "$",
            "invalidProductPayload",
            "The Product Master contains a value that cannot be represented as JSON.",
        )];
    };
    let mut issues = Vec::new();
    for field in [
        "subtype",
        "motorTechnology",
        "specifications",
        "performanceCurves",
    ] {
        let working = working_payload.get(field).unwrap_or(&Value::Null);
        let accepted = normalized_payload.get(field).unwrap_or(&Value::Null);
        if working != accepted && !override_covers(field, active_override_paths) {
            issues.push(issue(
                field,
                "sourceOwnedFieldMismatch",
                "A source-owned field differs from accepted staging without an active temporary override.",
            ));
        }
    }
    issues
}

pub fn issues_as_errors(issues: Vec<ValidationIssue>) -> BTreeMap<String, Vec<String>> {
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    for issue in issues {
        let detail = format!("{}: {}", issue.code, issue.detail);
        let values = errors.entry(issue.field_path).or_default();
        if !values.contains(&detail) {
            values.push(detail);
        }
    }
    errors
}

pub fn immutable_revision_payload(product: &Product) -> Result<Value, ValidationIssue> {
    let mut payload = serde_json::to_value(product).map_err(|_| {
        issue(
            "$",
            "invalidProductPayload",
            "The Product Master contains a value that cannot be represented as JSON.",
        )
    })?;
    if let Some(object) = payload.as_object_mut() {
        // Publication bookkeeping is mutable state on the working aggregate;
        // it is not part of the immutable editorial/Product Master revision.
        object.remove("status");
        object.remove("publishedRevision");
        object.remove("updatedAt");
    }
    Ok(payload)
}

pub fn workflow_error(
    field_path: impl Into<String>,
    code: impl Into<String>,
    detail: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue {
        field_path: field_path.into(),
        code: code.into(),
        detail: detail.into(),
    }
}

fn validate_text(field: &str, value: &str, maximum: usize, issues: &mut Vec<ValidationIssue>) {
    if value.trim().is_empty()
        || value.len() > maximum
        || value.chars().any(|character| character.is_control())
    {
        issues.push(issue(
            field,
            "invalidText",
            &format!("{field} must be printable text containing 1 to {maximum} characters."),
        ));
    }
}

fn validate_route_text(
    field: &str,
    value: &str,
    maximum: usize,
    predicate: fn(&str) -> bool,
    issues: &mut Vec<ValidationIssue>,
) {
    if value.len() > maximum || !predicate(value) {
        issues.push(issue(
            field,
            "invalidRouteIdentity",
            &format!("{field} is not a valid normalized route identity."),
        ));
    }
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.contains("--")
}

fn is_locale(value: &str) -> bool {
    let mut parts = value.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    (2..=3).contains(&language.len())
        && language.bytes().all(|byte| byte.is_ascii_lowercase())
        && parts.all(|part| {
            (part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_uppercase()))
                || (part.len() == 4
                    && part.as_bytes()[0].is_ascii_uppercase()
                    && part.as_bytes()[1..]
                        .iter()
                        .all(|byte| byte.is_ascii_lowercase()))
        })
}

fn issue(field_path: &str, code: &str, detail: &str) -> ValidationIssue {
    workflow_error(field_path, code, detail)
}

fn override_covers(field: &str, active_override_paths: &[String]) -> bool {
    active_override_paths.iter().any(|path| {
        let path = path.trim();
        path == field
            || path
                .strip_prefix(field)
                .is_some_and(|suffix| suffix.starts_with('.') || suffix.starts_with('['))
    })
}

fn deduplicate(issues: Vec<ValidationIssue>) -> Vec<ValidationIssue> {
    let mut seen = std::collections::BTreeSet::new();
    issues
        .into_iter()
        .filter(|issue| seen.insert((issue.field_path.clone(), issue.code.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use airtek_domain::models::{
        CurvePoint, FactState, PerformanceCurve, ProductFamily, SpecValue,
    };

    fn valid_product() -> Product {
        Product {
            id: Uuid::new_v4(),
            stable_id: "AT-TRACEABLE-001".into(),
            model: Some("TRACEABLE-MODEL".into()),
            slug: "traceable-model".into(),
            locale: "en".into(),
            family: ProductFamily::Axial,
            subtype: None,
            motor_technology: None,
            title: "Traceable product".into(),
            summary: None,
            seo: Default::default(),
            sort_order: 0,
            related_content_ids: Vec::new(),
            media_gallery: Vec::new(),
            specifications: vec![SpecValue {
                key: "ratedVoltage".into(),
                label: "Rated voltage".into(),
                value: Some(json!(230)),
                unit: Some("V".into()),
                operating_condition: None,
                state: FactState::Verified,
                source_reference: Some("feishu:record:voltage".into()),
            }],
            source_facts: Vec::new(),
            performance_curves: vec![PerformanceCurve {
                airflow_unit: "m3/h".into(),
                pressure_unit: "Pa".into(),
                speed_rpm: Some(1200),
                density_kg_m3: Some(1.2),
                voltage: Some("230 V, 50 Hz".into()),
                test_method: Some("Controlled Product Master test".into()),
                source_reference: "feishu:curve:1".into(),
                state: FactState::Verified,
                points: vec![
                    CurvePoint {
                        airflow: 0.0,
                        pressure: 300.0,
                    },
                    CurvePoint {
                        airflow: 100.0,
                        pressure: 200.0,
                    },
                ],
            }],
            source_snapshot_id: Uuid::new_v4(),
            source_revision: "source-revision-1".into(),
            current_revision: 1,
            published_revision: None,
            status: PublicationStatus::Draft,
            indexable: true,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn accepts_a_traceable_validated_product_master() {
        assert!(validate_product_master(&valid_product()).is_empty());
    }

    #[test]
    fn rejects_pending_facts_invalid_units_conditions_and_curves() {
        let mut product = valid_product();
        product.specifications[0].state = FactState::PendingVerification;
        product.specifications[0].unit = Some("volts-ish".into());
        product.specifications[0].operating_condition = Some("\n".into());
        product.performance_curves[0].test_method = None;
        product.performance_curves[0].points[1].airflow = 0.0;

        let issues = validate_product_master(&product);
        for code in [
            "pendingVerification",
            "unsupportedUnit",
            "invalidOperatingCondition",
            "testMethodRequired",
            "nonIncreasingAirflow",
        ] {
            assert!(issues.iter().any(|issue| issue.code == code), "{code}");
        }
    }

    #[test]
    fn accepted_staging_must_match_source_identity() {
        let product = valid_product();
        let mut payload = serde_json::to_value(&product).unwrap();
        payload["sourceRevision"] = json!("different-revision");
        let issues = validate_accepted_staging_payload(&product, &payload);
        assert!(issues
            .iter()
            .any(|issue| issue.code == "stagingIdentityMismatch"));
    }

    #[test]
    fn source_owned_differences_require_a_matching_active_override() {
        let mut product = valid_product();
        let accepted = serde_json::to_value(&product).unwrap();
        product.specifications[0].value = Some(json!(240));
        assert!(validate_source_owned_alignment(&product, &accepted, &[])
            .iter()
            .any(|issue| issue.code == "sourceOwnedFieldMismatch"));
        assert!(validate_source_owned_alignment(
            &product,
            &accepted,
            &["specifications.ratedVoltage".into()]
        )
        .is_empty());
    }
}
