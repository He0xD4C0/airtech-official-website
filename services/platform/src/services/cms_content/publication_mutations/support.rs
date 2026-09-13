use std::{collections::BTreeMap, time::Duration};

use serde::Serialize;
use serde_json::Value;
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};

use super::{AttemptError, MAX_SERIALIZABLE_ATTEMPTS, PUBLICATION_LOCK_KEY};
use crate::{
    error::ApiError,
    models::{ContentDraftV2, ContentRecordV2, ContentRevisionKindV2},
    services::{
        cms_content::MutationMetadata,
        cms_publication_dependencies::{
            ActiveContentDependent, DependencyBlockingIssue, ValidatedPublicationDependencies,
        },
        cms_templates,
    },
};

pub(super) async fn serializable_transaction(
    pool: &PgPool,
) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(PUBLICATION_LOCK_KEY)
        .execute(&mut *transaction)
        .await?;
    Ok(transaction)
}

pub(super) async fn lock_source(
    transaction: &mut Transaction<'_, Postgres>,
    id: uuid::Uuid,
) -> Result<ContentRecordV2, AttemptError> {
    let row = sqlx::query(
        r#"SELECT entry.id,entry.status,entry.latest_revision,
                  entry.cms_published_revision,entry.cms_created_at,draft.document,
                  draft.draft_version,draft.updated_at,draft.updated_by
           FROM content_entries entry
           JOIN content_drafts draft ON draft.content_id=entry.id
           WHERE entry.id=$1 FOR UPDATE OF entry,draft"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| AttemptError::Api(ApiError::not_found("Content entry was not found.")))?;
    decode_locked_record(&row)
}

fn decode_locked_record(row: &PgRow) -> Result<ContentRecordV2, AttemptError> {
    let document: ContentDraftV2 =
        serde_json::from_value(row.try_get("document")?).map_err(|error| {
            tracing::error!(%error, "stored CMS content draft is invalid");
            ApiError::service_unavailable("Stored content draft data is invalid.")
        })?;
    let draft_version: i64 = row.try_get("draft_version")?;
    if document.draft_version != draft_version {
        return Err(AttemptError::Api(ApiError::service_unavailable(
            "Stored content draft version does not match its row version.",
        )));
    }
    let latest_revision: i64 = row.try_get("latest_revision")?;
    Ok(ContentRecordV2 {
        id: row.try_get("id")?,
        status: serde_json::from_value(Value::String(row.try_get("status")?)).map_err(|error| {
            tracing::error!(%error, "stored CMS content status is invalid");
            ApiError::service_unavailable("Stored content status data is invalid.")
        })?,
        draft: document,
        latest_revision: (latest_revision > 0).then_some(latest_revision),
        published_revision: row.try_get("cms_published_revision")?,
        created_at: row.try_get("cms_created_at")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: row.try_get("updated_by")?,
    })
}

pub(super) fn ensure_expected_version(
    record: &ContentRecordV2,
    expected: i64,
    action: &str,
) -> Result<(), ApiError> {
    if record.draft.draft_version == expected {
        Ok(())
    } else {
        Err(version_conflict(action))
    }
}

pub(super) fn version_conflict(action: &str) -> ApiError {
    ApiError::conflict(format!(
        "The content draft changed; reload before {action}."
    ))
}

pub(super) fn dependency_validation_error(plan: &ValidatedPublicationDependencies) -> ApiError {
    dependency_issues_error(&plan.blocking_issues)
}

pub(super) fn dependency_issues_error(issues: &[DependencyBlockingIssue]) -> ApiError {
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    for issue in issues {
        errors
            .entry(issue.path.clone())
            .or_default()
            .push(issue_message(issue));
    }
    let issues = issues
        .iter()
        .filter_map(|issue| serde_json::to_value(issue).ok())
        .collect();
    ApiError::validation(errors)
        .with_code(crate::error::CONTENT_DEPENDENCY_CONFLICT)
        .with_issues(issues)
}

fn issue_message(issue: &DependencyBlockingIssue) -> String {
    let code = serde_json::to_value(issue.code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "dependencyBlocked".into());
    format!("{code}: {}", issue.detail)
}

pub(super) fn active_dependency_conflict(blockers: &[ActiveContentDependent]) -> ApiError {
    let listing = blockers
        .iter()
        .map(|blocker| {
            format!(
                "source={}; revision={}; path={}",
                blocker.source_content_id, blocker.source_revision, blocker.reference_path
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    ApiError::new(
        axum::http::StatusCode::CONFLICT,
        "Unpublish blocked",
        format!("Active published content still depends on this content: {listing}"),
    )
    .with_code(crate::error::CONTENT_DEPENDENCY_CONFLICT)
}

pub(super) async fn insert_publication_revision(
    transaction: &mut Transaction<'_, Postgres>,
    id: uuid::Uuid,
    revision: i64,
    draft: &ContentDraftV2,
    reason: &str,
    metadata: &MutationMetadata,
) -> Result<(), sqlx::Error> {
    let document =
        serde_json::to_value(draft).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,
            revision_kind,reason,created_by,created_at)
           VALUES ($1,$2,NULL,$3,$4,$5,$6,$7,now())"#,
    )
    .bind(id)
    .bind(revision)
    .bind(document)
    .bind(draft.draft_version)
    .bind(enum_label(ContentRevisionKindV2::Publish))
    .bind(reason.trim())
    .bind(&metadata.actor)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(super) async fn write_public_route(
    transaction: &mut Transaction<'_, Postgres>,
    record: &ContentRecordV2,
) -> Result<Option<String>, AttemptError> {
    let draft = &record.draft;
    let definition = cms_templates::template_definition(draft.template_key).ok_or_else(|| {
        AttemptError::Api(ApiError::internal(
            "The CMS content template is not registered.",
        ))
    })?;
    if !definition.routable {
        sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
            .bind(record.id)
            .execute(&mut **transaction)
            .await?;
        return Ok(None);
    }
    let path =
        cms_templates::canonical_path(&draft.locale, draft.template_key, draft.slug.as_deref())
            .ok_or_else(|| {
                AttemptError::Api(ApiError::validation(BTreeMap::from([(
                    "/slug".into(),
                    vec!["No canonical public route can be derived for this content.".into()],
                )])))
            })?;
    if let Some((entity_type, entity_id)) =
        conflicting_route_owner(transaction, &path, record.id, &draft.locale).await?
    {
        return Err(AttemptError::Api(route_conflict(
            &path,
            &entity_type,
            entity_id,
        )));
    }
    sqlx::query("SAVEPOINT cms_public_route_upsert")
        .execute(&mut **transaction)
        .await?;
    let write = sqlx::query(
        r#"INSERT INTO public_routes
           (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
           VALUES ($1,'content',$2,$3,$4,$5,now())
           ON CONFLICT (entity_type,entity_id,locale) DO UPDATE SET
             canonical_path=EXCLUDED.canonical_path,
             indexable=EXCLUDED.indexable,
             updated_at=EXCLUDED.updated_at"#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(record.id)
    .bind(&draft.locale)
    .bind(&path)
    .bind(draft.seo.indexable && !draft.is_placeholder)
    .execute(&mut **transaction)
    .await;
    match write {
        Ok(_) => {
            sqlx::query("RELEASE SAVEPOINT cms_public_route_upsert")
                .execute(&mut **transaction)
                .await?;
            Ok(Some(path))
        }
        Err(error) if is_canonical_path_conflict(&error) => {
            sqlx::query("ROLLBACK TO SAVEPOINT cms_public_route_upsert")
                .execute(&mut **transaction)
                .await?;
            sqlx::query("RELEASE SAVEPOINT cms_public_route_upsert")
                .execute(&mut **transaction)
                .await?;
            // A SERIALIZABLE snapshot cannot see the row that committed after
            // this transaction began. Retry with a fresh snapshot so the
            // normal owner query can return a stable, useful 409.
            Err(AttemptError::RouteClaimRace(path))
        }
        Err(error) => Err(AttemptError::Database(error)),
    }
}

async fn conflicting_route_owner(
    transaction: &mut Transaction<'_, Postgres>,
    path: &str,
    content_id: uuid::Uuid,
    locale: &str,
) -> Result<Option<(String, uuid::Uuid)>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT entity_type,entity_id FROM public_routes
           WHERE canonical_path=$1
             AND NOT (entity_type='content' AND entity_id=$2 AND locale=$3)
           LIMIT 1"#,
    )
    .bind(path)
    .bind(content_id)
    .bind(locale)
    .fetch_optional(&mut **transaction)
    .await
}

fn is_canonical_path_conflict(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|database| {
        database.code().as_deref() == Some("23505")
            && database.constraint() == Some("public_routes_canonical_path_key")
    })
}

fn route_conflict(path: &str, entity_type: &str, entity_id: uuid::Uuid) -> ApiError {
    ApiError::conflict(format!(
        "The public path {path} is already published by {entity_type} {entity_id}."
    ))
}

pub(super) async fn insert_audit_row(
    transaction: &mut Transaction<'_, Postgres>,
    metadata: &MutationMetadata,
    action: &str,
    entity_id: uuid::Uuid,
    before: &ContentRecordV2,
    after: &ContentRecordV2,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,$3,'content',$4,$5,$6,$7,$8,now())"#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&metadata.actor)
    .bind(action)
    .bind(entity_id)
    .bind(to_protocol_value(before)?)
    .bind(to_protocol_value(after)?)
    .bind(reason.trim())
    .bind(metadata.request_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(super) async fn insert_outbox(
    transaction: &mut Transaction<'_, Postgres>,
    topic: &str,
    aggregate_id: uuid::Uuid,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO outbox_events
           (id,topic,aggregate_type,aggregate_id,payload)
           VALUES ($1,$2,'content',$3,$4)"#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(topic)
    .bind(aggregate_id)
    .bind(payload)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(super) fn json_value<T: Serialize>(value: &T) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| ApiError::internal("CMS response serialization failed."))
}

fn to_protocol_value<T: Serialize>(value: &T) -> Result<Value, sqlx::Error> {
    serde_json::to_value(value).map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn enum_label<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(super) fn should_retry(error: &AttemptError, attempt: usize) -> bool {
    attempt < MAX_SERIALIZABLE_ATTEMPTS
        && (matches!(error, AttemptError::RouteClaimRace(_))
            || matches!(error, AttemptError::Database(error) if is_retryable_database_error(error)))
}

pub(super) fn is_retryable_database_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| matches!(database.code().as_deref(), Some("40001" | "40P01")))
}

pub(super) fn attempt_error(error: AttemptError) -> ApiError {
    match error {
        AttemptError::Api(error) => error,
        AttemptError::Database(error) => {
            retry_exhausted_or_database(error, MAX_SERIALIZABLE_ATTEMPTS)
        }
        AttemptError::RouteClaimRace(path) => {
            ApiError::conflict(format!("The public path {path} was claimed concurrently."))
        }
    }
}

pub(super) fn retry_exhausted_or_database(error: sqlx::Error, attempt: usize) -> ApiError {
    if is_retryable_database_error(&error) && attempt >= MAX_SERIALIZABLE_ATTEMPTS {
        tracing::warn!(attempt, error = %error, "CMS publication serialization retries exhausted");
        serialization_retry_conflict()
    } else {
        error.into()
    }
}

fn serialization_retry_conflict() -> ApiError {
    ApiError::conflict(
        "The publication changed concurrently after the retry limit; retry with the same Idempotency-Key.",
    )
}

pub(super) async fn retry_backoff(attempt: usize) {
    tokio::time::sleep(Duration::from_millis((attempt as u64) * 10)).await;
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    #[test]
    fn serialization_retry_exhaustion_is_a_conflict() {
        assert_eq!(
            super::serialization_retry_conflict().status(),
            StatusCode::CONFLICT
        );
    }
}
