use serde_json::{Map, Value};

use crate::models::{SeoInput, TiptapDocument, TiptapRootType};

use super::{
    conversion::{blocking, issue, IssueContext},
    types::{CmsPreflightIssue, CmsPreflightIssueCode, CmsPreflightSeverity},
};

pub(super) struct DocumentParts {
    pub page_slots: Option<Map<String, Value>>,
    pub attrs: Option<Map<String, Value>>,
    pub document: Option<TiptapDocument>,
}

pub(super) fn extract_document(
    payload: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<DocumentParts> {
    let Some(body_value) = payload.get("body") else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidTiptapDocument,
            "body",
            "Content payload is missing its body wrapper.",
        ));
        return None;
    };
    let Some(body) = body_value.as_object() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidTiptapDocument,
            "body",
            "Content body must be an object.",
        ));
        return None;
    };
    check_keys(body, &["schemaVersion", "doc"], context, "body", issues);
    if body.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidTiptapDocument,
            "body.schemaVersion",
            "Legacy Tiptap wrapper must use schema version 1.",
        ));
    }
    let Some(doc) = body.get("doc").and_then(Value::as_object) else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidTiptapDocument,
            "body.doc",
            "Content body.doc must be an object.",
        ));
        return None;
    };
    check_keys(
        doc,
        &["type", "attrs", "content"],
        context,
        "body.doc",
        issues,
    );
    if doc.get("type").and_then(Value::as_str) != Some("doc") {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidTiptapDocument,
            "body.doc.type",
            "Tiptap root type must be doc.",
        ));
    }
    let Some(content) = doc.get("content").and_then(Value::as_array) else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidTiptapDocument,
            "body.doc.content",
            "Tiptap root content must be an array.",
        ));
        return None;
    };
    let attrs = match doc.get("attrs") {
        None => None,
        Some(Value::Object(value)) => Some(value.clone()),
        Some(_) => {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::InvalidTiptapDocument,
                "body.doc.attrs",
                "Tiptap root attrs must be an object.",
            ));
            None
        }
    };
    let nested = attrs
        .as_ref()
        .and_then(|attrs| attrs.get("pageSlots"))
        .and_then(Value::as_object)
        .cloned();
    if attrs
        .as_ref()
        .is_some_and(|attrs| attrs.contains_key("pageSlots"))
        && nested.is_none()
    {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::EmbeddedPageSlots,
            "body.doc.attrs.pageSlots",
            "Embedded pageSlots must be an object.",
        ));
    }
    let root = match payload.get("pageSlots") {
        None => None,
        Some(Value::Object(value)) => Some(value.clone()),
        Some(_) => {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::EmbeddedPageSlots,
                "pageSlots",
                "Root pageSlots must be an object.",
            ));
            None
        }
    };
    if nested.is_some() || root.is_some() {
        issues.push(issue(
            CmsPreflightSeverity::Warning,
            CmsPreflightIssueCode::EmbeddedPageSlots,
            context.source,
            Some(context.entity_id),
            Some(context.revision),
            "body.doc.attrs.pageSlots",
            "Legacy pageSlots are extracted into typed V2 composition and removed from Tiptap.",
        ));
    }
    if nested.is_some() && root.is_some() && nested != root {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::EmbeddedPageSlots,
            "pageSlots",
            "Root and Tiptap pageSlots differ; precedence would be unsafe.",
        ));
    }
    Some(DocumentParts {
        page_slots: nested.or(root),
        attrs,
        document: Some(TiptapDocument {
            node_type: TiptapRootType::Doc,
            content: content.clone(),
        }),
    })
}

pub(super) fn check_payload(
    payload: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    const ALLOWED: &[&str] = &[
        "id",
        "kind",
        "slug",
        "locale",
        "title",
        "summary",
        "body",
        "seo",
        "status",
        "isPlaceholder",
        "currentRevision",
        "publishedRevision",
        "scheduledFor",
        "updatedAt",
        "pageSlots",
    ];
    check_keys(payload, ALLOWED, context, "payload", issues);
    if payload
        .get("summary")
        .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidLegacyPayload,
            "payload.summary",
            "Content summary must be a string or null.",
        ));
    }
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
            CmsPreflightIssueCode::InvalidLegacyPayload,
            &format!("{path}.{key}"),
            "Unknown legacy field has no lossless CMS V2 mapping.",
        ));
    }
}

pub(super) fn check_document_attrs(
    kind: &str,
    attrs: Option<&Map<String, Value>>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let allowed: &[&str] = match kind {
        "article" => &[
            "pageSlots",
            "author",
            "authorType",
            "publishedAt",
            "category",
        ],
        "download" => &[
            "pageSlots",
            "version",
            "applicableModels",
            "resourceType",
            "fileDescription",
            "downloadUrl",
            "fileStatus",
        ],
        _ => &["pageSlots"],
    };
    for key in attrs
        .into_iter()
        .flat_map(|attrs| attrs.keys())
        .filter(|key| !allowed.contains(&key.as_str()))
    {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            &format!("body.doc.attrs.{key}"),
            "Unknown document attribute has no V2 type-field mapping.",
        ));
    }
}

pub(super) fn convert_seo(
    value: Option<&Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> SeoInput {
    let object = match value {
        None => None,
        Some(Value::Object(value)) => Some(value),
        Some(_) => {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::InvalidLegacyPayload,
                "seo",
                "Legacy SEO must be an object when present.",
            ));
            None
        }
    };
    if let Some(object) = object {
        for key in object.keys().filter(|key| {
            !matches!(
                key.as_str(),
                "title" | "description" | "canonicalPath" | "indexable"
            )
        }) {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::TypeFieldMismatch,
                &format!("seo.{key}"),
                "Unknown SEO field has no V2 mapping.",
            ));
        }
        if object
            .get("canonicalPath")
            .is_some_and(|value| !value.is_null())
        {
            issues.push(issue(
                CmsPreflightSeverity::Warning,
                CmsPreflightIssueCode::InvalidLegacyPayload,
                context.source,
                Some(context.entity_id),
                Some(context.revision),
                "seo.canonicalPath",
                "Legacy canonicalPath is explicitly removed; V2 derives canonical routes on the server.",
            ));
        }
        for key in ["title", "description"] {
            if object
                .get(key)
                .is_some_and(|value| !value.is_null() && !value.is_string())
            {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::InvalidLegacyPayload,
                    &format!("seo.{key}"),
                    "SEO text must be a string or null.",
                ));
            }
        }
        if object
            .get("indexable")
            .is_some_and(|value| !value.is_boolean())
        {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::InvalidLegacyPayload,
                "seo.indexable",
                "SEO indexable must be a boolean.",
            ));
        }
        if object
            .get("canonicalPath")
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::InvalidLegacyPayload,
                "seo.canonicalPath",
                "Legacy canonicalPath must be a string or null.",
            ));
        }
    }
    SeoInput {
        title: object
            .and_then(|value| value.get("title"))
            .and_then(Value::as_str)
            .map(Into::into),
        description: object
            .and_then(|value| value.get("description"))
            .and_then(Value::as_str)
            .map(Into::into),
        indexable: object
            .and_then(|value| value.get("indexable"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        social_image: None,
    }
}
