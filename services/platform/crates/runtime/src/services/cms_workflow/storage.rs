use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::Value;
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::auth::AdminPrincipal;
use crate::error::ApiError;
use crate::services::cms_templates::{validate_content_draft, CmsTemplateValidationPhase};
use crate::state::AppState;
use airtek_domain::models::{CmsDraftState, CmsPrivateDraft, CmsPublishedContent, ContentDraftV2};

use super::storage_support::*;

pub async fn get_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
) -> Result<CmsPrivateDraft, ApiError> {
    let query = format!(
        "SELECT {DRAFT_COLUMNS} FROM cms_drafts draft WHERE draft.draft_id=$1 AND {}",
        visibility_sql(2)
    );
    sqlx::query(&query)
        .bind(draft_id)
        .bind(principal.user_id)
        .bind(is_super_admin(principal))
        .bind(principal.has_permission("content.publish"))
        .fetch_optional(&state.pool)
        .await?
        .map(|row| decode_draft(&row))
        .transpose()?
        .ok_or_else(|| ApiError::not_found("Private draft was not found."))
}

pub async fn get_published(
    state: &AppState,
    content_id: Uuid,
) -> Result<CmsPublishedContent, ApiError> {
    sqlx::query(
        r#"SELECT content_id,document,publication_version,published_by,
                  published_at,updated_at
           FROM cms_published_content WHERE content_id=$1"#,
    )
    .bind(content_id)
    .fetch_optional(&state.pool)
    .await?
    .map(|row| decode_published(&row))
    .transpose()?
    .ok_or_else(|| ApiError::not_found("Published content was not found."))
}

pub async fn create_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    mut document: ContentDraftV2,
) -> Result<CmsPrivateDraft, ApiError> {
    document.draft_version = 1;
    validate_document(&document, CmsTemplateValidationPhase::Structural)?;
    let draft_id = Uuid::new_v4();
    let content_id = Uuid::new_v4();
    let now = Utc::now();
    let mut transaction = state.pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,is_placeholder,data_origin,template_key,created_at)
           VALUES ($1,$2,$3,$4,$5,'editorial',$6,$7)"#,
    )
    .bind(content_id)
    .bind(enum_label(document.kind))
    .bind(document.slug.as_deref().unwrap_or(""))
    .bind(&document.locale)
    .bind(document.is_placeholder)
    .bind(enum_label(document.template_key))
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(map_identity_conflict)?;
    insert_draft(
        &mut transaction,
        draft_id,
        content_id,
        principal.user_id,
        &document,
        0,
        now,
    )
    .await?;
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.create",
        draft_id,
        1,
    )
    .await?;
    transaction.commit().await?;
    Ok(CmsPrivateDraft {
        draft_id,
        content_id,
        owner_user_id: Some(principal.user_id),
        document,
        draft_version: 1,
        base_publication_version: 0,
        state: CmsDraftState::Editing,
        rejection_reason: None,
        created_at: now,
        updated_at: now,
    })
}

pub async fn copy_published_to_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    content_id: Uuid,
) -> Result<CmsPrivateDraft, ApiError> {
    let now = Utc::now();
    let draft_id = Uuid::new_v4();
    let mut transaction = state.pool.begin().await?;
    let published = lock_published(&mut transaction, content_id).await?;
    if let Some(existing) = sqlx::query(&format!(
        "SELECT {DRAFT_COLUMNS} FROM cms_drafts draft \
         WHERE draft.content_id=$1 AND draft.owner_user_id=$2"
    ))
    .bind(content_id)
    .bind(principal.user_id)
    .fetch_optional(&mut *transaction)
    .await?
    {
        transaction.rollback().await?;
        return decode_draft(&existing);
    }
    let mut document = published.document;
    document.draft_version = 1;
    insert_draft(
        &mut transaction,
        draft_id,
        content_id,
        principal.user_id,
        &document,
        published.publication_version,
        now,
    )
    .await?;
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.copy",
        draft_id,
        1,
    )
    .await?;
    transaction.commit().await?;
    Ok(CmsPrivateDraft {
        draft_id,
        content_id,
        owner_user_id: Some(principal.user_id),
        document,
        draft_version: 1,
        base_publication_version: published.publication_version,
        state: CmsDraftState::Editing,
        rejection_reason: None,
        created_at: now,
        updated_at: now,
    })
}

pub async fn save_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
    expected_version: i64,
    mut document: ContentDraftV2,
) -> Result<CmsPrivateDraft, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let before = lock_owned_draft(&mut transaction, principal.user_id, draft_id).await?;
    if before.state != CmsDraftState::Editing {
        return Err(ApiError::conflict(
            "A pending review draft cannot be edited.",
        ));
    }
    if before.draft_version != expected_version {
        return Err(version_conflict());
    }
    if document.kind != before.document.kind
        || document.locale != before.document.locale
        || document.template_key != before.document.template_key
    {
        return Err(ApiError::conflict(
            "Draft identity fields cannot be changed.",
        ));
    }
    document.draft_version = expected_version + 1;
    validate_document(&document, CmsTemplateValidationPhase::Structural)?;
    let now = Utc::now();
    let updated = sqlx::query(
        r#"UPDATE cms_drafts SET document=$2,draft_version=$3,
                  rejection_reason=NULL,updated_at=$4
           WHERE draft_id=$1 AND draft_version=$5 AND state='editing'"#,
    )
    .bind(draft_id)
    .bind(json(&document)?)
    .bind(document.draft_version)
    .bind(now)
    .bind(expected_version)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(version_conflict());
    }
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.save",
        draft_id,
        document.draft_version,
    )
    .await?;
    transaction.commit().await?;
    Ok(CmsPrivateDraft {
        document,
        draft_version: expected_version + 1,
        rejection_reason: None,
        updated_at: now,
        ..before
    })
}

pub async fn set_draft_shares(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
    mut user_ids: Vec<Uuid>,
) -> Result<CmsPrivateDraft, ApiError> {
    user_ids.sort_unstable();
    user_ids.dedup();
    if user_ids.len() > 100 || user_ids.contains(&principal.user_id) {
        return Err(ApiError::bad_request("Share with at most 100 other users."));
    }
    let mut transaction = state.pool.begin().await?;
    let draft = lock_owned_draft(&mut transaction, principal.user_id, draft_id).await?;
    sqlx::query("DELETE FROM cms_draft_shares WHERE draft_id=$1")
        .bind(draft_id)
        .execute(&mut *transaction)
        .await?;
    for user_id in user_ids {
        let inserted = sqlx::query(
            r#"INSERT INTO cms_draft_shares
               (draft_id,shared_with_user_id,shared_by_user_id)
               SELECT $1,id,$3 FROM users WHERE id=$2 AND status='active'"#,
        )
        .bind(draft_id)
        .bind(user_id)
        .bind(principal.user_id)
        .execute(&mut *transaction)
        .await?;
        if inserted.rows_affected() != 1 {
            return Err(ApiError::bad_request("Every shared user must be active."));
        }
    }
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.share",
        draft_id,
        draft.draft_version,
    )
    .await?;
    transaction.commit().await?;
    Ok(draft)
}

pub async fn claim_unassigned_draft(
    state: &AppState,
    principal: &AdminPrincipal,
    draft_id: Uuid,
) -> Result<CmsPrivateDraft, ApiError> {
    if !is_super_admin(principal) {
        return Err(ApiError::forbidden(
            "Only Super Admin can claim an unassigned draft.",
        ));
    }
    let mut transaction = state.pool.begin().await?;
    let updated = sqlx::query(
        r#"UPDATE cms_drafts SET owner_user_id=$2,updated_at=now()
           WHERE draft_id=$1 AND owner_user_id IS NULL
           RETURNING draft_id,content_id,owner_user_id,document,draft_version,
                     base_publication_version,state,rejection_reason,created_at,updated_at"#,
    )
    .bind(draft_id)
    .bind(principal.user_id)
    .fetch_optional(&mut *transaction)
    .await?
    .map(|row| decode_draft(&row))
    .transpose()?
    .ok_or_else(|| ApiError::conflict("The draft is assigned already or no longer exists."))?;
    insert_audit(
        &mut transaction,
        principal,
        "contentDraft.claim",
        draft_id,
        updated.draft_version,
    )
    .await?;
    transaction.commit().await?;
    Ok(updated)
}

pub(super) async fn lock_owned_draft(
    transaction: &mut Transaction<'_, Postgres>,
    owner_user_id: Uuid,
    draft_id: Uuid,
) -> Result<CmsPrivateDraft, ApiError> {
    sqlx::query(&format!(
        "SELECT {DRAFT_COLUMNS} FROM cms_drafts draft \
         WHERE draft.draft_id=$1 AND draft.owner_user_id=$2 FOR UPDATE"
    ))
    .bind(draft_id)
    .bind(owner_user_id)
    .fetch_optional(&mut **transaction)
    .await?
    .map(|row| decode_draft(&row))
    .transpose()?
    .ok_or_else(|| ApiError::not_found("Owned private draft was not found."))
}

pub(super) async fn lock_published(
    transaction: &mut Transaction<'_, Postgres>,
    content_id: Uuid,
) -> Result<CmsPublishedContent, ApiError> {
    sqlx::query(
        r#"SELECT content_id,document,publication_version,published_by,
                  published_at,updated_at
           FROM cms_published_content WHERE content_id=$1 FOR UPDATE"#,
    )
    .bind(content_id)
    .fetch_optional(&mut **transaction)
    .await?
    .map(|row| decode_published(&row))
    .transpose()?
    .ok_or_else(|| ApiError::not_found("Published content was not found."))
}

pub(super) async fn insert_draft(
    transaction: &mut Transaction<'_, Postgres>,
    draft_id: Uuid,
    content_id: Uuid,
    owner_user_id: Uuid,
    document: &ContentDraftV2,
    base_publication_version: i64,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO cms_drafts
           (draft_id,content_id,owner_user_id,document,draft_version,
            base_publication_version,state,created_at,updated_at)
           VALUES ($1,$2,$3,$4,1,$5,'editing',$6,$6)"#,
    )
    .bind(draft_id)
    .bind(content_id)
    .bind(owner_user_id)
    .bind(json(document)?)
    .bind(base_publication_version)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(super) fn decode_draft(row: &PgRow) -> Result<CmsPrivateDraft, ApiError> {
    let document: ContentDraftV2 = decode_json(row.try_get("document")?)?;
    let version = row.try_get("draft_version")?;
    if document.draft_version != version {
        return Err(ApiError::service_unavailable(
            "Stored draft version is inconsistent.",
        ));
    }
    Ok(CmsPrivateDraft {
        draft_id: row.try_get("draft_id")?,
        content_id: row.try_get("content_id")?,
        owner_user_id: row.try_get("owner_user_id")?,
        document,
        draft_version: version,
        base_publication_version: row.try_get("base_publication_version")?,
        state: decode_json(Value::String(row.try_get("state")?))?,
        rejection_reason: row.try_get("rejection_reason")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub(super) fn decode_published(row: &PgRow) -> Result<CmsPublishedContent, ApiError> {
    Ok(CmsPublishedContent {
        content_id: row.try_get("content_id")?,
        document: decode_json(row.try_get("document")?)?,
        publication_version: row.try_get("publication_version")?,
        published_by: row.try_get("published_by")?,
        published_at: row.try_get("published_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub(super) async fn insert_audit(
    transaction: &mut Transaction<'_, Postgres>,
    principal: &AdminPrincipal,
    action: &str,
    entity_id: Uuid,
    version: i64,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,current_version,request_id,occurred_at)
           VALUES ($1,$2,$3,'contentDraft',$4,NULL,NULL,NULL,$5,$6,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(&principal.email)
    .bind(action)
    .bind(entity_id)
    .bind(version)
    .bind(Uuid::new_v4())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn validate_document(
    document: &ContentDraftV2,
    phase: CmsTemplateValidationPhase,
) -> Result<(), ApiError> {
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    for issue in validate_content_draft(document, phase) {
        errors.entry(issue.path).or_default().push(issue.message);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

pub(super) fn validate_publish_document(document: &ContentDraftV2) -> Result<(), ApiError> {
    validate_document(document, CmsTemplateValidationPhase::Publish)
}

fn decode_json<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|_| ApiError::service_unavailable("Stored CMS document is invalid."))
}

fn json<T: serde::Serialize>(value: &T) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|_| ApiError::internal("CMS serialization failed."))
}

fn enum_label<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn map_identity_conflict(error: sqlx::Error) -> ApiError {
    if error
        .as_database_error()
        .is_some_and(|value| value.code().as_deref() == Some("23505"))
    {
        ApiError::conflict("A content entry already uses this kind, slug and locale.")
    } else {
        error.into()
    }
}

fn version_conflict() -> ApiError {
    ApiError::conflict("The private draft changed; reload before saving.")
}
