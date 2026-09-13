use serde::Serialize;
use sqlx::Row;

use super::*;
use crate::{
    models::{
        CmsPublicationStatusV2, ContentPublicationAction, ContentPublicationReadiness,
        PublicationReadinessIssue,
    },
    services::{cms_publication_dependencies as dependencies, cms_templates},
};

pub async fn publication_readiness(
    state: &AppState,
    id: Uuid,
    may_publish: bool,
) -> Result<ContentPublicationReadiness, ApiError> {
    let record = get_content(state, id).await?;
    let mut issues = cms_templates::validate_content_draft(
        &record.draft,
        cms_templates::CmsTemplateValidationPhase::Publish,
    )
    .into_iter()
    .map(|issue| PublicationReadinessIssue {
        code: enum_value(issue.code),
        path: issue.path,
        detail: issue.message,
        failed_gate: Some("templateValidation".into()),
        target_id: None,
    })
    .collect::<Vec<_>>();

    let pool = require_postgres(state)?;
    let mut connection = pool.acquire().await?;
    let extracted = dependencies::extract_draft(&record.draft);
    let plan = dependencies::validate_extracted(&mut connection, extracted).await?;
    issues.extend(
        plan.blocking_issues
            .into_iter()
            .map(|issue| PublicationReadinessIssue {
                code: enum_value(issue.code),
                path: issue.path,
                detail: issue.detail,
                failed_gate: Some(enum_value(issue.failed_gate)),
                target_id: issue.target_id,
            }),
    );
    if let Some(issue) = canonical_route_issue(&mut connection, &record).await? {
        issues.push(issue);
    }
    if record.status == CmsPublicationStatusV2::Archived {
        issues.push(PublicationReadinessIssue {
            code: "contentArchived".into(),
            path: "status".into(),
            detail: "Archived content must be restored to draft before publishing.".into(),
            failed_gate: Some("publicationState".into()),
            target_id: None,
        });
    }
    issues.sort_by(|left, right| {
        (&left.path, &left.code, left.target_id).cmp(&(&right.path, &right.code, right.target_id))
    });
    issues.dedup();

    let mut allowed_actions = vec![ContentPublicationAction::Save];
    if issues.is_empty() && may_publish {
        allowed_actions.push(ContentPublicationAction::Publish);
    }
    if record.status == CmsPublicationStatusV2::Published && may_publish {
        allowed_actions.push(ContentPublicationAction::Unpublish);
    }
    Ok(ContentPublicationReadiness {
        content_id: record.id,
        draft_version: record.draft.draft_version,
        ready: issues.is_empty(),
        issues,
        allowed_actions,
    })
}

async fn canonical_route_issue(
    connection: &mut sqlx::PgConnection,
    record: &ContentRecordV2,
) -> Result<Option<PublicationReadinessIssue>, ApiError> {
    let Some(path) = cms_templates::canonical_path(
        &record.draft.locale,
        record.draft.template_key,
        record.draft.slug.as_deref(),
    ) else {
        return Ok(None);
    };
    let conflict = sqlx::query(
        r#"SELECT entity_type,entity_id FROM public_routes
           WHERE canonical_path=$1 AND NOT (entity_type='content' AND entity_id=$2)
           LIMIT 1"#,
    )
    .bind(&path)
    .bind(record.id)
    .fetch_optional(connection)
    .await?;
    Ok(conflict.map(|row| {
        let owner_type: String = row.get("entity_type");
        let owner_id: Uuid = row.get("entity_id");
        PublicationReadinessIssue {
            code: "canonicalPathConflict".into(),
            path: "slug".into(),
            detail: format!("Canonical path {path} is already owned by {owner_type} {owner_id}."),
            failed_gate: Some("canonicalRoute".into()),
            target_id: Some(owner_id),
        }
    }))
}

fn enum_value(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}
