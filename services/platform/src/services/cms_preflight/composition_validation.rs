use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::models::CmsContentKind;

use super::{
    conversion::{blocking, issue, IssueContext},
    types::{CmsPreflightIssue, CmsPreflightIssueCode, CmsPreflightSeverity},
};

pub(super) fn check_object_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: IssueContext,
    path: &str,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for key in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            &format!("{path}.{key}"),
            "Legacy block contains a field with no V2 mapping.",
        ));
    }
}

pub(super) fn check_optional_strings(
    object: &Map<String, Value>,
    fields: &[&str],
    context: IssueContext,
    path: &str,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for field in fields {
        if object
            .get(*field)
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::UnknownBlock,
                &format!("{path}.{field}"),
                "Block text must be a string or null.",
            ));
        }
    }
}

pub(super) fn optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Into::into)
}

pub(super) fn warn_server_derived_slots(
    slots: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for key in ["breadcrumbs", "categories", "category"] {
        if slots.contains_key(key) {
            issues.push(issue(CmsPreflightSeverity::Warning, CmsPreflightIssueCode::InvalidLegacyPayload, context.source, Some(context.entity_id), Some(context.revision), &format!("pageSlots.{key}"), "Field is explicitly removed because V2 derives it from routes or validated projections."));
        }
    }
}

pub(super) fn check_unknown_slots(
    slots: &Map<String, Value>,
    kind: CmsContentKind,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let known = [
        "templateKey",
        "eyebrow",
        "hero",
        "breadcrumbs",
        "sections",
        "relationships",
        "primaryCta",
        "categories",
        "category",
        "items",
        "columns",
        "legalLinks",
        "media",
        "featureGrid",
        "evidence",
        "faqCollection",
        "downloadAsset",
        "contactBlock",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    for key in slots.keys().filter(|key| !known.contains(key.as_str())) {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            &format!("pageSlots.{key}"),
            "Unknown page slot cannot be migrated safely.",
        ));
    }
    if slots.contains_key("items") && kind != CmsContentKind::Navigation {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            "pageSlots.items",
            "items is only valid for Navigation content.",
        ));
    }
    for key in ["columns", "legalLinks"] {
        if slots.contains_key(key) && kind != CmsContentKind::Footer {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::UnknownBlock,
                &format!("pageSlots.{key}"),
                "This slot is only valid for Footer content.",
            ));
        }
    }
}
