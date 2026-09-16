use chrono::Utc;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{
    storage::{decode_draft, insert_audit, lock_owned_draft, validate_publish_document},
    storage_support::DRAFT_COLUMNS,
};
use crate::{
    auth::AdminPrincipal,
    error::ApiError,
    models::{CmsDraftState, CmsPrivateDraft, CmsPublishResult, CmsSubmitResult},
    services::{cms_content::publish_current_route, cms_publication_dependencies as dependencies},
    state::AppState,
};

enum PublishAttempt {
    Published(CmsPublishResult),
    Stale,
}

pub async fn submit_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
    expected_version: i64,
) -> Result<CmsSubmitResult, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let draft = lock_owned_draft(&mut transaction, principal.user_id, draft_id).await?;
    if draft.state != CmsDraftState::Editing {
        return Err(ApiError::conflict(
            "The private draft is already pending review.",
        ));
    }
    if draft.draft_version != expected_version {
        return Err(ApiError::conflict(
            "The private draft changed; save and reload before submitting.",
        ));
    }
    validate_publish_document(&draft.document)?;
    let review_required = content_review_required(&mut transaction).await?;
    if !review_required || principal.has_permission("content.publish") {
        let result = publish_locked(&mut transaction, principal, &draft).await?;
        transaction.commit().await?;
        return match result {
            PublishAttempt::Published(publication) => Ok(CmsSubmitResult {
                status: "published".into(),
                draft: None,
                publication: Some(publication),
            }),
            PublishAttempt::Stale => Err(stale_conflict()),
        };
    }
    let updated = sqlx::query(
        r#"UPDATE cms_drafts SET state='pendingReview',rejection_reason=NULL,updated_at=now()
           WHERE draft_id=$1 AND draft_version=$2 AND state='editing'"#,
    )
    .bind(draft_id)
    .bind(expected_version)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "The private draft changed while submitting.",
        ));
    }
    sqlx::query(
        r#"INSERT INTO cms_review_queue(draft_id,submitted_by_user_id)
           VALUES ($1,$2)"#,
    )
    .bind(draft_id)
    .bind(principal.user_id)
    .execute(&mut *transaction)
    .await?;
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.submit",
        draft_id,
        expected_version,
    )
    .await?;
    transaction.commit().await?;
    Ok(CmsSubmitResult {
        status: "pendingReview".into(),
        draft: Some(CmsPrivateDraft {
            state: CmsDraftState::PendingReview,
            rejection_reason: None,
            ..draft
        }),
        publication: None,
    })
}

pub async fn withdraw_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
) -> Result<CmsPrivateDraft, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let draft = lock_owned_draft(&mut transaction, principal.user_id, draft_id).await?;
    if draft.state != CmsDraftState::PendingReview {
        return Err(ApiError::conflict(
            "Only a pending review draft can be withdrawn.",
        ));
    }
    sqlx::query(
        "UPDATE cms_drafts SET state='editing',rejection_reason=NULL,updated_at=now() WHERE draft_id=$1",
    )
    .bind(draft_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM cms_review_queue WHERE draft_id=$1")
        .bind(draft_id)
        .execute(&mut *transaction)
        .await?;
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.withdraw",
        draft_id,
        draft.draft_version,
    )
    .await?;
    transaction.commit().await?;
    Ok(CmsPrivateDraft {
        state: CmsDraftState::Editing,
        rejection_reason: None,
        ..draft
    })
}

pub async fn approve_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
) -> Result<CmsPublishResult, ApiError> {
    require_reviewer(principal)?;
    let mut transaction = state.pool.begin().await?;
    let draft = lock_review_draft(&mut transaction, draft_id).await?;
    validate_publish_document(&draft.document)?;
    let result = publish_locked(&mut transaction, principal, &draft).await?;
    transaction.commit().await?;
    match result {
        PublishAttempt::Published(publication) => Ok(publication),
        PublishAttempt::Stale => Err(stale_conflict()),
    }
}

pub async fn reject_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
    reason: String,
) -> Result<CmsPrivateDraft, ApiError> {
    require_reviewer(principal)?;
    let reason = reason.trim();
    if !(1..=2000).contains(&reason.chars().count()) {
        return Err(ApiError::bad_request(
            "Rejection reason must contain 1 to 2000 characters.",
        ));
    }
    let mut transaction = state.pool.begin().await?;
    let draft = lock_review_draft(&mut transaction, draft_id).await?;
    sqlx::query(
        r#"UPDATE cms_drafts SET state='editing',rejection_reason=$2,updated_at=now()
           WHERE draft_id=$1"#,
    )
    .bind(draft_id)
    .bind(reason)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM cms_review_queue WHERE draft_id=$1")
        .bind(draft_id)
        .execute(&mut *transaction)
        .await?;
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.reject",
        draft_id,
        draft.draft_version,
    )
    .await?;
    transaction.commit().await?;
    Ok(CmsPrivateDraft {
        state: CmsDraftState::Editing,
        rejection_reason: Some(reason.to_owned()),
        ..draft
    })
}

async fn lock_review_draft(
    transaction: &mut Transaction<'_, Postgres>,
    draft_id: Uuid,
) -> Result<CmsPrivateDraft, ApiError> {
    sqlx::query(&format!(
        r#"SELECT {DRAFT_COLUMNS} FROM cms_review_queue queue
           JOIN cms_drafts draft ON draft.draft_id=queue.draft_id
           WHERE queue.draft_id=$1 AND draft.state='pendingReview'
           FOR UPDATE OF draft,queue"#
    ))
    .bind(draft_id)
    .fetch_optional(&mut **transaction)
    .await?
    .map(|row| decode_draft(&row))
    .transpose()?
    .ok_or_else(|| ApiError::not_found("Pending review draft was not found."))
}

async fn publish_locked(
    transaction: &mut Transaction<'_, Postgres>,
    principal: &AdminPrincipal,
    draft: &CmsPrivateDraft,
) -> Result<PublishAttempt, ApiError> {
    let current_version = sqlx::query_scalar::<_, i64>(
        r#"SELECT publication_version FROM cms_published_content
           WHERE content_id=$1 FOR UPDATE"#,
    )
    .bind(draft.content_id)
    .fetch_optional(&mut **transaction)
    .await?
    .unwrap_or(0);
    if current_version != draft.base_publication_version {
        sqlx::query(
            "UPDATE cms_drafts SET state='editing',rejection_reason=NULL,updated_at=now() WHERE draft_id=$1",
        )
        .bind(draft.draft_id)
        .execute(&mut **transaction)
        .await?;
        sqlx::query("DELETE FROM cms_review_queue WHERE draft_id=$1")
            .bind(draft.draft_id)
            .execute(&mut **transaction)
            .await?;
        insert_audit(
            transaction,
            principal,
            "contentDraft.stale",
            draft.draft_id,
            draft.draft_version,
        )
        .await?;
        return Ok(PublishAttempt::Stale);
    }
    let extracted = dependencies::extract_draft(&draft.document);
    dependencies::lock_targets(transaction, &extracted.lock_targets).await?;
    let plan = dependencies::validate_extracted(transaction, extracted).await?;
    if !plan.is_complete() {
        let issues = plan
            .blocking_issues
            .iter()
            .filter_map(|issue| serde_json::to_value(issue).ok())
            .collect();
        return Err(ApiError::validation(Default::default())
            .with_code(crate::error::CONTENT_DEPENDENCY_CONFLICT)
            .with_issues(issues));
    }
    let publication_version = current_version + 1;
    let now = Utc::now();
    sqlx::query(
        r#"INSERT INTO cms_published_content
           (content_id,document,publication_version,published_by,published_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,$5)
           ON CONFLICT (content_id) DO UPDATE SET
             document=EXCLUDED.document,
             publication_version=EXCLUDED.publication_version,
             published_by=EXCLUDED.published_by,
             published_at=EXCLUDED.published_at,
             updated_at=EXCLUDED.updated_at"#,
    )
    .bind(draft.content_id)
    .bind(
        serde_json::to_value(&draft.document)
            .map_err(|_| ApiError::internal("CMS serialization failed."))?,
    )
    .bind(publication_version)
    .bind(principal.user_id)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    dependencies::replace_current(
        transaction,
        draft.content_id,
        publication_version,
        &principal.email,
        &plan,
    )
    .await?;
    sqlx::query("UPDATE content_entries SET slug=$2,is_placeholder=$3 WHERE id=$1")
        .bind(draft.content_id)
        .bind(draft.document.slug.as_deref().unwrap_or(""))
        .bind(draft.document.is_placeholder)
        .execute(&mut **transaction)
        .await
        .map_err(|error| {
            if error
                .as_database_error()
                .is_some_and(|value| value.code().as_deref() == Some("23505"))
            {
                ApiError::conflict(
                    "A published content route already uses this canonical identity.",
                )
            } else {
                error.into()
            }
        })?;
    publish_current_route(transaction, draft.content_id, &draft.document).await?;
    sqlx::query(
        r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
           VALUES ($1,'public.content.published','content',$2,$3)"#,
    )
    .bind(Uuid::new_v4())
    .bind(draft.content_id)
    .bind(serde_json::json!({
        "entityId": draft.content_id,
        "revision": publication_version,
        "locale": draft.document.locale,
    }))
    .execute(&mut **transaction)
    .await?;
    insert_audit(
        transaction,
        principal,
        "content.publish",
        draft.content_id,
        publication_version,
    )
    .await?;
    sqlx::query("DELETE FROM cms_drafts WHERE draft_id=$1")
        .bind(draft.draft_id)
        .execute(&mut **transaction)
        .await?;
    Ok(PublishAttempt::Published(CmsPublishResult {
        content_id: draft.content_id,
        publication_version,
        published_at: now,
    }))
}

async fn content_review_required(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<bool, ApiError> {
    let value = sqlx::query_scalar::<_, Value>(
        "SELECT value FROM app_settings WHERE key='contentReviewRequired'",
    )
    .fetch_optional(&mut **transaction)
    .await?
    .unwrap_or(Value::Bool(true));
    value.as_bool().ok_or_else(|| {
        ApiError::service_unavailable("contentReviewRequired must be a boolean setting.")
    })
}

fn require_reviewer(principal: &AdminPrincipal) -> Result<(), ApiError> {
    if principal.has_permission("content.publish") {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "The `content.publish` permission is required.",
        ))
    }
}

fn stale_conflict() -> ApiError {
    ApiError::conflict(
        "Published content changed after this private draft was copied. The draft returned to editing.",
    )
    .with_code("content_publication_stale")
}
