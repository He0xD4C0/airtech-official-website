use serde_json::Value;
use uuid::Uuid;

use crate::{
    models::{
        CmsContentKind, ContentDraftV2, ContentTemplateKey, ContentTypeFields, PageComposition,
        SeoInput, CMS_V2_SCHEMA_VERSION,
    },
    services::cms_templates::{validate_content_draft, CmsTemplateValidationPhase},
};

use super::{
    conversion::issue,
    general_information_fields::convert_fields,
    types::{
        CmsPreflightIssue, CmsPreflightIssueCode, CmsPreflightRecord, CmsPreflightRecordRole,
        CmsPreflightSeverity, CmsPreflightSource,
    },
};

pub(super) struct GeneralInformationSource {
    pub id: Uuid,
    pub revision: i64,
    pub role: CmsPreflightRecordRole,
    pub scope: String,
    pub locale: String,
    pub is_published_revision: bool,
    pub is_placeholder: bool,
    pub scheduled: bool,
    pub payload: Value,
}

pub(super) fn convert_general_information(
    source: GeneralInformationSource,
) -> (Option<CmsPreflightRecord>, Vec<CmsPreflightIssue>) {
    let mut issues = Vec::new();
    if source.scheduled {
        push(
            &mut issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::ScheduledPublicationUnsupported,
            source.id,
            source.revision,
            "status",
            "Scheduled General Information requires an owner decision before migration.",
        );
    }
    if source.scope != "site" {
        push(
            &mut issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            source.id,
            source.revision,
            "scope",
            "Only the site General Information scope has a V2 contract.",
        );
    }
    let Some(payload) = source.payload.as_object() else {
        push(
            &mut issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::InvalidLegacyPayload,
            source.id,
            source.revision,
            "payload",
            "General Information payload is not a JSON object.",
        );
        return (None, issues);
    };
    let fields = convert_fields(
        payload,
        source.is_placeholder,
        source.id,
        source.revision,
        &mut issues,
    );
    let candidate = ContentDraftV2 {
        schema_version: CMS_V2_SCHEMA_VERSION,
        kind: CmsContentKind::GeneralInformation,
        locale: source.locale.clone(),
        template_key: ContentTemplateKey::GeneralInformation,
        title: "General Information".into(),
        slug: None,
        summary: None,
        is_placeholder: source.is_placeholder,
        type_fields: ContentTypeFields::GeneralInformation(Box::new(fields)),
        body: None,
        composition: PageComposition { blocks: Vec::new() },
        seo: SeoInput::default(),
        relations: Vec::new(),
        draft_version: source.revision,
    };
    validate_candidate(&source, &candidate, &mut issues);
    let blocker = issues
        .iter()
        .any(|issue| issue.severity == CmsPreflightSeverity::Blocking);
    let record = (!blocker).then_some(CmsPreflightRecord {
        entity_id: source.id,
        source_revision: source.revision,
        role: source.role,
        candidate,
    });
    (record, issues)
}

fn validate_candidate(
    source: &GeneralInformationSource,
    candidate: &ContentDraftV2,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let structural = validate_content_draft(candidate, CmsTemplateValidationPhase::Structural);
    for validation in &structural {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::RevisionConversionFailed,
            source.id,
            source.revision,
            &validation.path,
            &validation.message,
        );
    }
    let severity = if source.is_published_revision && !source.is_placeholder {
        CmsPreflightSeverity::Blocking
    } else {
        CmsPreflightSeverity::Warning
    };
    for validation in validate_content_draft(candidate, CmsTemplateValidationPhase::Publish)
        .into_iter()
        .filter(|value| !structural.contains(value))
    {
        push(
            issues,
            severity,
            CmsPreflightIssueCode::RevisionConversionFailed,
            source.id,
            source.revision,
            &validation.path,
            &validation.message,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    issues: &mut Vec<CmsPreflightIssue>,
    severity: CmsPreflightSeverity,
    code: CmsPreflightIssueCode,
    id: Uuid,
    revision: i64,
    path: &str,
    message: &str,
) {
    issues.push(issue(
        severity,
        code,
        CmsPreflightSource::GeneralInformation,
        Some(id),
        Some(revision),
        path,
        message,
    ));
}
