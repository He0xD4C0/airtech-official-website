use super::*;
use crate::services::cms_templates::{validate_content_draft, CmsTemplateValidationPhase};

pub(super) fn validate_create(draft: &ContentDraftV2) -> Result<(), ApiError> {
    if draft.draft_version != 1 {
        return Err(ApiError::validation(BTreeMap::from([(
            "draftVersion".into(),
            vec!["A new content draft must start at version 1.".into()],
        )])));
    }
    validate(draft, CmsTemplateValidationPhase::Structural)
}

pub(super) fn validate_save(
    before: &ContentDraftV2,
    draft: &ContentDraftV2,
    expected: i64,
) -> Result<(), ApiError> {
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    if draft.draft_version != expected {
        errors.insert(
            "draftVersion".into(),
            vec!["The request draftVersion must match If-Match.".into()],
        );
    }
    for (path, changed) in [
        ("kind", before.kind != draft.kind),
        ("locale", before.locale != draft.locale),
        ("templateKey", before.template_key != draft.template_key),
    ] {
        if changed {
            errors.insert(
                path.into(),
                vec!["This content identity field is immutable.".into()],
            );
        }
    }
    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }
    validate(draft, CmsTemplateValidationPhase::Structural)
}

pub(super) fn validate_snapshot(
    draft: &ContentDraftV2,
    intent: ContentSnapshotIntent,
    reason: &str,
) -> Result<(), ApiError> {
    validate_reason(reason)?;
    let phase = match intent {
        ContentSnapshotIntent::Manual => CmsTemplateValidationPhase::Structural,
        ContentSnapshotIntent::Publish => CmsTemplateValidationPhase::Publish,
    };
    validate(draft, phase)
}

pub(super) fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if (10..=2000).contains(&reason.trim().chars().count()) {
        Ok(())
    } else {
        Err(ApiError::validation(BTreeMap::from([(
            "reason".into(),
            vec!["Reason must contain 10 to 2000 characters.".into()],
        )])))
    }
}

fn validate(draft: &ContentDraftV2, phase: CmsTemplateValidationPhase) -> Result<(), ApiError> {
    let issues = validate_content_draft(draft, phase);
    if issues.is_empty() {
        return Ok(());
    }
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    for issue in issues {
        errors.entry(issue.path).or_default().push(issue.message);
    }
    Err(ApiError::validation(errors))
}
