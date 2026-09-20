//! DevTools-only adapter that sends fixtures through the canonical CMS publish transaction.

use chrono::Utc;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{
    storage::{insert_audit, insert_draft, validate_publish_document},
    workflow::{publish_locked, PublishAttempt},
};
use crate::{
    auth::AdminPrincipal,
    error::ApiError,
    models::{CmsDraftState, CmsPrivateDraft, CmsPublishResult, ContentDraftV2},
};

pub(crate) async fn publish_development_fixture(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    actor_email: &str,
    content_id: Uuid,
    document: &ContentDraftV2,
) -> Result<CmsPublishResult, ApiError> {
    validate_publish_document(document)?;
    let draft_id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,is_placeholder,data_origin,template_key,created_at)
           VALUES ($1,$2,$3,$4,true,'developmentFixture',$5,$6)"#,
    )
    .bind(content_id)
    .bind(enum_label(document.kind))
    .bind(document.slug.as_deref().unwrap_or(""))
    .bind(&document.locale)
    .bind(enum_label(document.template_key))
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    insert_draft(
        transaction,
        draft_id,
        content_id,
        actor_user_id,
        document,
        0,
        now,
    )
    .await?;
    let principal = fixture_principal(actor_user_id, actor_email);
    insert_audit(transaction, &principal, "contentDraft.create", draft_id, 1).await?;
    let draft = CmsPrivateDraft {
        draft_id,
        content_id,
        owner_user_id: Some(actor_user_id),
        document: document.clone(),
        draft_version: 1,
        base_publication_version: 0,
        state: CmsDraftState::Editing,
        rejection_reason: None,
        created_at: now,
        updated_at: now,
    };
    match publish_locked(transaction, &principal, &draft).await? {
        PublishAttempt::Published(result) => Ok(result),
        PublishAttempt::Stale => Err(ApiError::internal(
            "A newly created development fixture unexpectedly became stale.",
        )),
    }
}

fn fixture_principal(user_id: Uuid, email: &str) -> AdminPrincipal {
    AdminPrincipal {
        user_id,
        display_name: "Development public-site fixture".into(),
        email: email.into(),
        role: "super-admin".into(),
        permissions: vec!["content.publish".into()],
        session_id: Uuid::nil(),
        session_token_hash: Vec::new(),
        csrf_hash: Vec::new(),
        totp_enabled: true,
    }
}

fn enum_label<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
