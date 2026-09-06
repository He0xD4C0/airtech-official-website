use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use crate::models::{
    CaseStudyTypeFields, CmsContentKind, ContentTypeFields, DownloadTypeFields,
    EditorialTypeFields, FaqTypeFields, FooterColumn, FooterTypeFields, LegalTypeFields,
    NavigationItem, NavigationTypeFields, TaxonomyTypeFields,
};

use super::{
    conversion::{blocking, issue, IssueContext},
    support::{link_target, stable_id},
    types::{CmsPreflightIssue, CmsPreflightIssueCode, CmsPreflightSeverity},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn convert_type_fields(
    kind: CmsContentKind,
    legacy_kind: &str,
    slug: &str,
    attrs: Option<&Map<String, Value>>,
    slots: Option<&Map<String, Value>>,
    seed: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<ContentTypeFields> {
    let attrs = attrs.cloned().unwrap_or_default();
    let result = match kind {
        CmsContentKind::Home => ContentTypeFields::Home,
        CmsContentKind::Page => ContentTypeFields::Page,
        CmsContentKind::Solution => ContentTypeFields::Solution(TaxonomyTypeFields {
            key: Some(slug.into()),
        }),
        CmsContentKind::Technology => ContentTypeFields::Technology(TaxonomyTypeFields {
            key: Some(slug.into()),
        }),
        CmsContentKind::Article => {
            ContentTypeFields::Article(editorial_from_attrs(&attrs, context, issues))
        }
        CmsContentKind::News => ContentTypeFields::News(editorial_from_seed(seed)),
        CmsContentKind::Faq => ContentTypeFields::Faq(FaqTypeFields { items: Vec::new() }),
        CmsContentKind::CaseStudy => ContentTypeFields::CaseStudy(CaseStudyTypeFields {
            industry: None,
            location: None,
        }),
        CmsContentKind::Download => {
            ContentTypeFields::Download(download_fields(&attrs, seed, context, issues))
        }
        CmsContentKind::Company => ContentTypeFields::Company,
        CmsContentKind::Legal => ContentTypeFields::Legal(LegalTypeFields {
            effective_date: None,
        }),
        CmsContentKind::Navigation => ContentTypeFields::Navigation(NavigationTypeFields {
            items: navigation_items(
                slots.and_then(|value| value.get("items")),
                context,
                "items",
                issues,
            ),
        }),
        CmsContentKind::Footer => ContentTypeFields::Footer(FooterTypeFields {
            columns: footer_columns(
                slots.and_then(|value| value.get("columns")),
                context,
                issues,
            ),
            legal_links: navigation_items(
                slots.and_then(|value| value.get("legalLinks")),
                context,
                "legalLinks",
                issues,
            ),
        }),
        CmsContentKind::GeneralInformation => return None,
    };
    if legacy_kind == "article"
        && kind == CmsContentKind::Page
        && attrs.keys().any(|key| key != "pageSlots")
    {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            "body.doc.attrs",
            "Article metadata cannot be attached to an Article Index page.",
        ));
    }
    Some(result)
}

fn editorial_from_attrs(
    attrs: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> EditorialTypeFields {
    if attrs.contains_key("authorType") {
        if !matches!(
            attrs.get("authorType").and_then(Value::as_str),
            Some("Person" | "Organization")
        ) {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::TypeFieldMismatch,
                "body.doc.attrs.authorType",
                "Article authorType must be Person or Organization.",
            ));
        }
        issues.push(issue(
            CmsPreflightSeverity::Warning,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context.source,
            Some(context.entity_id),
            Some(context.revision),
            "body.doc.attrs.authorType",
            "V2 stores the reviewed author display name without the legacy schema.org hint.",
        ));
    }
    for field in ["author", "category"] {
        if attrs
            .get(field)
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::TypeFieldMismatch,
                &format!("body.doc.attrs.{field}"),
                "Article metadata must be a string or null.",
            ));
        }
    }
    let publication_at = text(attrs, "publishedAt").and_then(|value| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d")
                    .map(|date| date.and_hms_opt(0, 0, 0).expect("midnight").and_utc())
            })
            .map_err(|_| {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    "body.doc.attrs.publishedAt",
                    "Article publishedAt must include an unambiguous RFC3339 offset.",
                ));
            })
            .ok()
    });
    EditorialTypeFields {
        category: text(attrs, "category"),
        author_display_name: text(attrs, "author"),
        publication_at,
        cover: None,
        featured: false,
    }
}

fn download_fields(
    attrs: &Map<String, Value>,
    seed: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> DownloadTypeFields {
    for field in ["version", "resourceType", "fileDescription", "downloadUrl"] {
        if attrs
            .get(field)
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::TypeFieldMismatch,
                &format!("body.doc.attrs.{field}"),
                "Download metadata must be a string or null.",
            ));
        }
    }
    if attrs
        .get("downloadUrl")
        .is_some_and(|value| !value.is_null())
    {
        issues.push(issue(
            if seed.contains_key("asset") {
                CmsPreflightSeverity::Warning
            } else {
                CmsPreflightSeverity::Blocking
            },
            CmsPreflightIssueCode::MissingMediaVersion,
            context.source,
            Some(context.entity_id),
            Some(context.revision),
            "body.doc.attrs.downloadUrl",
            "Legacy downloadUrl is removed; CMS V2 derives the delivery URL from the mapped media version.",
        ));
    }
    validate_file_status(attrs.get("fileStatus"), seed, context, issues);
    DownloadTypeFields {
        version_label: text(attrs, "version"),
        resource_type: text(attrs, "resourceType"),
        version_notes: None,
    }
}

fn validate_file_status(
    value: Option<&Value>,
    seed: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let Some(value) = value else { return };
    let Some(object) = value.as_object() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            "body.doc.attrs.fileStatus",
            "Download fileStatus must be an object.",
        ));
        return;
    };
    for key in object
        .keys()
        .filter(|key| !matches!(key.as_str(), "scan" | "access"))
    {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            &format!("body.doc.attrs.fileStatus.{key}"),
            "Unknown fileStatus field has no CMS V2 mapping.",
        ));
    }
    let actual_scan = seed.get("assetScanStatus").and_then(Value::as_str);
    let actual_access = seed.get("assetAccessLevel").and_then(Value::as_str);
    for (field, actual) in [("scan", actual_scan), ("access", actual_access)] {
        let legacy = object.get(field).and_then(Value::as_str);
        if object.contains_key(field) && legacy.is_none() {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::TypeFieldMismatch,
                &format!("body.doc.attrs.fileStatus.{field}"),
                "Download file status values must be strings.",
            ));
        } else if legacy.is_some() && legacy != actual {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::MissingMediaVersion,
                &format!("body.doc.attrs.fileStatus.{field}"),
                "Legacy file status disagrees with the referenced media asset.",
            ));
        }
    }
    issues.push(issue(
        CmsPreflightSeverity::Warning,
        CmsPreflightIssueCode::TypeFieldMismatch,
        context.source,
        Some(context.entity_id),
        Some(context.revision),
        "body.doc.attrs.fileStatus",
        "Legacy fileStatus is removed; CMS V2 derives availability from the media version.",
    ));
}

fn editorial_from_seed(seed: &Map<String, Value>) -> EditorialTypeFields {
    EditorialTypeFields {
        category: seed.get("category").and_then(Value::as_str).map(Into::into),
        author_display_name: seed
            .get("authorDisplayName")
            .and_then(Value::as_str)
            .map(Into::into),
        publication_at: seed
            .get("publicationAt")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
        cover: seed
            .get("cover")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
        featured: seed
            .get("featured")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn footer_columns(
    value: Option<&Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Vec<FooterColumn> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(columns) = value.as_array() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            "pageSlots.columns",
            "Footer columns must be an array.",
        ));
        return Vec::new();
    };
    columns
        .iter()
        .enumerate()
        .filter_map(|(index, column)| {
            let Some(object) = column.as_object() else {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    &format!("pageSlots.columns.{index}"),
                    "Footer column must be an object.",
                ));
                return None;
            };
            check_keys(
                object,
                &["title", "links"],
                context,
                &format!("pageSlots.columns.{index}"),
                issues,
            );
            let title = object
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(title) = title else {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    &format!("pageSlots.columns.{index}"),
                    "Footer column needs a title.",
                ));
                return None;
            };
            Some(FooterColumn {
                id: stable_id(context, &format!("columns.{index}")),
                title: title.into(),
                links: navigation_items(
                    object.get("links"),
                    context,
                    &format!("columns.{index}.links"),
                    issues,
                ),
            })
        })
        .collect()
}

fn navigation_items(
    value: Option<&Value>,
    context: IssueContext,
    path: &str,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Vec<NavigationItem> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(items) = value.as_array() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            &format!("pageSlots.{path}"),
            "Navigation items must be an array.",
        ));
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let item_path = format!("{path}.{index}");
            let Some(object) = item.as_object() else {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    &format!("pageSlots.{item_path}"),
                    "Navigation item must be an object.",
                ));
                return None;
            };
            check_keys(
                object,
                &["label", "href", "children"],
                context,
                &format!("pageSlots.{item_path}"),
                issues,
            );
            let label = object
                .get("label")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let Some(label) = label else {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    &item_path,
                    "Navigation item needs a label.",
                ));
                return None;
            };
            let target = object
                .get("href")
                .and_then(Value::as_str)
                .and_then(link_target);
            if object.get("href").is_some() && target.is_none() {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::MissingRelationTarget,
                    &format!("{item_path}.href"),
                    "Navigation target is unsafe or unsupported.",
                ));
            }
            let children = navigation_items(
                object.get("children"),
                context,
                &format!("{item_path}.children"),
                issues,
            );
            Some(NavigationItem {
                id: stable_id(context, &item_path),
                label: label.into(),
                target,
                children,
            })
        })
        .collect()
}

fn check_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: IssueContext,
    path: &str,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for key in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            &format!("{path}.{key}"),
            "Unknown navigation field has no CMS V2 mapping.",
        ));
    }
}

fn text(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Into::into)
}
