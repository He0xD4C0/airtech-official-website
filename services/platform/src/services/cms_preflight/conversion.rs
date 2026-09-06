use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    models::{
        CmsBodyPolicy, CmsContentKind, ContentDraftV2, ContentRelationReference,
        CMS_V2_SCHEMA_VERSION,
    },
    services::cms_templates::{
        template_definition, validate_content_draft, CmsTemplateValidationPhase,
    },
};

use super::{
    composition::convert_page_slots,
    document::{check_document_attrs, check_payload, convert_seo, extract_document},
    download::{merge_relations, model_relations, reconcile_download_asset},
    templates::{resolve_template, TemplateResolutionError},
    type_fields::convert_type_fields,
    types::*,
};

pub(super) struct ContentSource {
    pub entity_id: Uuid,
    pub revision: i64,
    pub role: CmsPreflightRecordRole,
    pub kind: String,
    pub slug: String,
    pub locale: String,
    pub title: String,
    pub is_published_revision: bool,
    pub is_placeholder: bool,
    pub allows_lossy_placeholder_cleanup: bool,
    pub scheduled: bool,
    pub payload: Value,
    pub current_relations: Vec<ContentRelationReference>,
    pub relation_history_unavailable: bool,
}

#[derive(Clone, Copy)]
pub(super) struct IssueContext {
    pub source: CmsPreflightSource,
    pub entity_id: Uuid,
    pub revision: i64,
}

pub(super) fn convert_content(
    source: ContentSource,
    seed_type_fields: Map<String, Value>,
) -> (Option<CmsPreflightRecord>, Vec<CmsPreflightIssue>) {
    let context = IssueContext {
        source: if source.role == CmsPreflightRecordRole::Working {
            CmsPreflightSource::Content
        } else {
            CmsPreflightSource::ContentRevision
        },
        entity_id: source.entity_id,
        revision: source.revision,
    };
    let mut issues = Vec::new();
    if source.scheduled {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::ScheduledPublicationUnsupported,
            "status",
            "Scheduled legacy content requires an explicit owner decision before migration.",
        ));
    }
    let Some(payload) = source.payload.as_object() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidLegacyPayload,
            "payload",
            "Content payload is not a JSON object.",
        ));
        return (None, issues);
    };
    check_payload(payload, context, &mut issues);
    if source.relation_history_unavailable {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::RelationHistoryUnavailable,
            "relations",
            "Mutable legacy relations cannot be reconstructed for this historical revision.",
        ));
    }
    let Some(document_parts) = extract_document(payload, context, &mut issues) else {
        return (None, issues);
    };
    let legacy_template = document_parts
        .page_slots
        .as_ref()
        .and_then(|slots| slots.get("templateKey"))
        .and_then(Value::as_str);
    let effective_template = match (&*source.kind, legacy_template, &*source.slug) {
        ("company", Some("company"), "about") => Some("about"),
        ("company", Some("company"), "contact") => Some("contact"),
        _ => legacy_template,
    };
    let (kind, template_key) = match resolve_template(&source.kind, effective_template) {
        Ok(value) => value,
        Err(error) => {
            let message = match error {
                TemplateResolutionError::Missing => {
                    "Legacy content has no templateKey and cannot be classified safely."
                }
                TemplateResolutionError::Unsupported => {
                    "Legacy content uses a templateKey absent from the V2 registry."
                }
                TemplateResolutionError::Incompatible => {
                    "Legacy kind and templateKey are incompatible."
                }
            };
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::UnsupportedTemplate,
                "body.doc.attrs.pageSlots.templateKey",
                message,
            ));
            return (None, issues);
        }
    };
    let definition = template_definition(template_key).expect("resolved template is registered");
    let mut document = document_parts.document;
    if definition.body_policy == CmsBodyPolicy::Forbidden {
        if document
            .as_ref()
            .is_some_and(|document| !document.content.is_empty())
        {
            if source.allows_lossy_placeholder_cleanup {
                issues.push(placeholder_cleanup_warning(
                    context,
                    CmsPreflightIssueCode::InvalidTiptapDocument,
                    "body",
                    "Development fixture body is removed because this template forbids body content.",
                ));
            } else {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::InvalidTiptapDocument,
                    "body",
                    "This V2 template forbids body content; non-empty legacy rich text cannot be discarded.",
                ));
            }
        } else {
            issues.push(issue(
                CmsPreflightSeverity::Warning,
                CmsPreflightIssueCode::InvalidTiptapDocument,
                context.source,
                Some(context.entity_id),
                Some(context.revision),
                "body",
                "The empty legacy body wrapper is omitted because this V2 template forbids body content.",
            ));
        }
        document = None;
    }
    check_document_attrs(
        &source.kind,
        document_parts.attrs.as_ref(),
        context,
        &mut issues,
    );
    let Some(type_fields) = convert_type_fields(
        kind,
        &source.kind,
        &source.slug,
        document_parts.attrs.as_ref(),
        document_parts.page_slots.as_ref(),
        &seed_type_fields,
        context,
        &mut issues,
    ) else {
        return (None, issues);
    };
    let (mut composition, mut relations) = convert_page_slots(
        document_parts.page_slots.as_ref(),
        kind,
        context,
        &document,
        source.allows_lossy_placeholder_cleanup,
        &mut issues,
    );
    if source.allows_lossy_placeholder_cleanup {
        let mut index = 0;
        composition.blocks.retain(|block| {
            let keep = definition.allowed_blocks.contains(&block.kind());
            if !keep {
                issues.push(placeholder_cleanup_warning(
                    context,
                    CmsPreflightIssueCode::RevisionConversionFailed,
                    &format!("composition.blocks.{index}.type"),
                    "Development fixture block is removed because it is not allowed by this template.",
                ));
            }
            index += 1;
            keep
        });
    }
    if kind == CmsContentKind::Download {
        relations.extend(model_relations(
            document_parts.attrs.as_ref(),
            &seed_type_fields,
            context,
            &mut issues,
        ));
    }
    merge_relations(&mut relations, source.current_relations.clone());
    if kind == CmsContentKind::Download {
        reconcile_download_asset(
            &mut composition,
            document_parts.attrs.as_ref(),
            &seed_type_fields,
            &source.title,
            context,
            &mut issues,
        );
    }
    let mut seo = convert_seo(payload.get("seo"), context, &mut issues);
    let forced_noindex = source.is_placeholder
        || matches!(
            template_key,
            crate::models::ContentTemplateKey::Compare | crate::models::ContentTemplateKey::Search
        );
    if forced_noindex && seo.indexable {
        issues.push(issue(
            CmsPreflightSeverity::Warning,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context.source,
            Some(context.entity_id),
            Some(context.revision),
            "seo.indexable",
            "CMS V2 hard-normalizes placeholder, Compare, and Search content to noindex.",
        ));
        seo.indexable = false;
    }
    let candidate = ContentDraftV2 {
        schema_version: CMS_V2_SCHEMA_VERSION,
        kind,
        locale: source.locale.clone(),
        template_key,
        title: source.title.clone(),
        slug: if matches!(kind, CmsContentKind::Navigation | CmsContentKind::Footer) {
            None
        } else {
            Some(source.slug.clone())
        },
        summary: payload
            .get("summary")
            .and_then(Value::as_str)
            .map(Into::into),
        is_placeholder: source.is_placeholder,
        type_fields,
        body: document,
        composition,
        seo,
        relations,
        draft_version: source.revision,
    };
    validate_candidate(&source, &candidate, context, &mut issues);
    let has_blocker = issues
        .iter()
        .any(|issue| issue.severity == CmsPreflightSeverity::Blocking);
    let record = (!has_blocker).then_some(CmsPreflightRecord {
        entity_id: source.entity_id,
        source_revision: source.revision,
        role: source.role,
        candidate,
    });
    (record, issues)
}

fn validate_candidate(
    source: &ContentSource,
    candidate: &ContentDraftV2,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let structural = validate_content_draft(candidate, CmsTemplateValidationPhase::Structural);
    for validation in &structural {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::RevisionConversionFailed,
            &validation.path,
            &validation.message,
        ));
    }
    let severity = if source.is_published_revision && !source.is_placeholder {
        CmsPreflightSeverity::Blocking
    } else {
        CmsPreflightSeverity::Warning
    };
    for validation in validate_content_draft(candidate, CmsTemplateValidationPhase::Publish)
        .into_iter()
        .filter(|candidate| !structural.contains(candidate))
    {
        issues.push(issue(
            severity,
            CmsPreflightIssueCode::RevisionConversionFailed,
            context.source,
            Some(context.entity_id),
            Some(context.revision),
            &validation.path,
            &validation.message,
        ));
    }
}

pub(super) fn blocking(
    context: IssueContext,
    code: CmsPreflightIssueCode,
    path: &str,
    message: &str,
) -> CmsPreflightIssue {
    issue(
        CmsPreflightSeverity::Blocking,
        code,
        context.source,
        Some(context.entity_id),
        Some(context.revision),
        path,
        message,
    )
}

fn placeholder_cleanup_warning(
    context: IssueContext,
    code: CmsPreflightIssueCode,
    path: &str,
    message: &str,
) -> CmsPreflightIssue {
    issue(
        CmsPreflightSeverity::Warning,
        code,
        context.source,
        Some(context.entity_id),
        Some(context.revision),
        path,
        message,
    )
}

pub(super) fn issue(
    severity: CmsPreflightSeverity,
    code: CmsPreflightIssueCode,
    source: CmsPreflightSource,
    entity_id: Option<Uuid>,
    revision: Option<i64>,
    path: &str,
    message: &str,
) -> CmsPreflightIssue {
    CmsPreflightIssue {
        severity,
        code,
        source,
        entity_id,
        revision,
        json_path: Some(path.into()),
        message: message.into(),
    }
}
