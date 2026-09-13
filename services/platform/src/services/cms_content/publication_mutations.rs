use std::collections::BTreeMap;

use axum::http::StatusCode;
use sqlx::{Postgres, Transaction};

use super::{validation::*, *};
use crate::services::cms_publication_dependencies as dependencies;

#[path = "publication_mutations/support.rs"]
mod support;
use support::*;

const MAX_SERIALIZABLE_ATTEMPTS: usize = 3;
const PUBLICATION_LOCK_KEY: &str = "airtek.cms.publication.v2";

enum AttemptError {
    Api(ApiError),
    Database(sqlx::Error),
    RouteClaimRace(String),
}

impl From<ApiError> for AttemptError {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl From<sqlx::Error> for AttemptError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

pub async fn publish_content(
    state: &AppState,
    id: Uuid,
    expected: i64,
    reason: String,
    metadata: MutationMetadata,
    idempotency: IdempotencyContext,
) -> Result<ContentRecordV2, ApiError> {
    let pool = require_postgres(state)?;
    for attempt in 1..=MAX_SERIALIZABLE_ATTEMPTS {
        let mut transaction = match serializable_transaction(pool).await {
            Ok(transaction) => transaction,
            Err(error)
                if is_retryable_database_error(&error) && attempt < MAX_SERIALIZABLE_ATTEMPTS =>
            {
                retry_backoff(attempt).await;
                continue;
            }
            Err(error) => return Err(retry_exhausted_or_database(error, attempt)),
        };
        let result = publish_attempt(&mut transaction, id, expected, &reason, &metadata).await;
        let record = match result {
            Ok(record) => record,
            Err(error) => {
                let retry = should_retry(&error, attempt);
                let _ = transaction.rollback().await;
                if retry {
                    retry_backoff(attempt).await;
                    continue;
                }
                return Err(attempt_error(error));
            }
        };
        let response = json_value(&record)?;
        match idempotency
            .stage_value_in_transaction(&mut transaction, &response, StatusCode::CREATED)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                let _ = transaction.rollback().await;
                return Err(ApiError::conflict(
                    "This Idempotency-Key is already active for another request.",
                ));
            }
            Err(error) => {
                let retry =
                    is_retryable_database_error(&error) && attempt < MAX_SERIALIZABLE_ATTEMPTS;
                let _ = transaction.rollback().await;
                if retry {
                    retry_backoff(attempt).await;
                    continue;
                }
                return Err(retry_exhausted_or_database(error, attempt));
            }
        }
        match transaction.commit().await {
            Ok(()) => {
                idempotency.finish_after_commit().await?;
                return Ok(record);
            }
            Err(error)
                if is_retryable_database_error(&error) && attempt < MAX_SERIALIZABLE_ATTEMPTS =>
            {
                retry_backoff(attempt).await;
            }
            Err(error) => return Err(retry_exhausted_or_database(error, attempt)),
        }
    }
    unreachable!("bounded publication loop always returns")
}

pub async fn unpublish_content(
    state: &AppState,
    id: Uuid,
    expected: i64,
    expected_published_revision: i64,
    reason: String,
    metadata: MutationMetadata,
    idempotency: IdempotencyContext,
) -> Result<ContentRecordV2, ApiError> {
    validate_unpublish_reason(&reason)?;
    if expected_published_revision <= 0 {
        return Err(ApiError::validation(BTreeMap::from([(
            "expectedPublishedRevision".into(),
            vec!["Expected published revision must be positive.".into()],
        )])));
    }
    let pool = require_postgres(state)?;
    for attempt in 1..=MAX_SERIALIZABLE_ATTEMPTS {
        let mut transaction = match serializable_transaction(pool).await {
            Ok(transaction) => transaction,
            Err(error)
                if is_retryable_database_error(&error) && attempt < MAX_SERIALIZABLE_ATTEMPTS =>
            {
                retry_backoff(attempt).await;
                continue;
            }
            Err(error) => return Err(retry_exhausted_or_database(error, attempt)),
        };
        let result = unpublish_attempt(
            &mut transaction,
            id,
            expected,
            expected_published_revision,
            &reason,
            &metadata,
        )
        .await;
        let record = match result {
            Ok(record) => record,
            Err(error) => {
                let retry = should_retry(&error, attempt);
                let _ = transaction.rollback().await;
                if retry {
                    retry_backoff(attempt).await;
                    continue;
                }
                return Err(attempt_error(error));
            }
        };
        let response = json_value(&record)?;
        match idempotency
            .stage_value_in_transaction(&mut transaction, &response, StatusCode::OK)
            .await
        {
            Ok(true) => {}
            Ok(false) => {
                let _ = transaction.rollback().await;
                return Err(ApiError::conflict(
                    "This Idempotency-Key is already active for another request.",
                ));
            }
            Err(error) => {
                let retry =
                    is_retryable_database_error(&error) && attempt < MAX_SERIALIZABLE_ATTEMPTS;
                let _ = transaction.rollback().await;
                if retry {
                    retry_backoff(attempt).await;
                    continue;
                }
                return Err(retry_exhausted_or_database(error, attempt));
            }
        }
        match transaction.commit().await {
            Ok(()) => {
                idempotency.finish_after_commit().await?;
                return Ok(record);
            }
            Err(error)
                if is_retryable_database_error(&error) && attempt < MAX_SERIALIZABLE_ATTEMPTS =>
            {
                retry_backoff(attempt).await;
            }
            Err(error) => return Err(retry_exhausted_or_database(error, attempt)),
        }
    }
    unreachable!("bounded unpublication loop always returns")
}

fn validate_unpublish_reason(reason: &str) -> Result<(), ApiError> {
    if (1..=2000).contains(&reason.trim().chars().count()) {
        Ok(())
    } else {
        Err(ApiError::validation(BTreeMap::from([(
            "reason".into(),
            vec!["Reason must contain 1 to 2000 characters.".into()],
        )])))
    }
}

async fn publish_attempt(
    transaction: &mut Transaction<'_, Postgres>,
    id: Uuid,
    expected: i64,
    reason: &str,
    metadata: &MutationMetadata,
) -> Result<ContentRecordV2, AttemptError> {
    let before = lock_source(transaction, id).await?;
    ensure_expected_version(&before, expected, "publishing")?;
    if before.status == CmsPublicationStatusV2::Archived {
        return Err(AttemptError::Api(
            ApiError::conflict(
                "Archived content must be restored to draft before it can be published.",
            )
            .with_code("content_archived"),
        ));
    }
    validate_snapshot(&before.draft, ContentSnapshotIntent::Publish, reason)?;

    let extracted = dependencies::extract_draft(&before.draft);
    dependencies::lock_targets(transaction, &extracted.lock_targets).await?;
    let plan = dependencies::validate_extracted(transaction, extracted).await?;
    if !plan.is_complete() {
        return Err(AttemptError::Api(dependency_validation_error(&plan)));
    }
    let revision = before.latest_revision.unwrap_or(0) + 1;
    insert_publication_revision(transaction, id, revision, &before.draft, reason, metadata).await?;
    dependencies::insert_snapshot(transaction, id, revision, &metadata.actor, &plan).await?;
    let updated = sqlx::query(
        r#"UPDATE content_entries
           SET latest_revision=$2,cms_published_revision=$2,status='published',
               cms_updated_by=$3
           WHERE id=$1 AND latest_revision=$4"#,
    )
    .bind(id)
    .bind(revision)
    .bind(&metadata.actor)
    .bind(before.latest_revision.unwrap_or(0))
    .execute(&mut **transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AttemptError::Api(version_conflict("publishing")));
    }
    let record = ContentRecordV2 {
        status: CmsPublicationStatusV2::Published,
        latest_revision: Some(revision),
        published_revision: Some(revision),
        ..before.clone()
    };
    let canonical_path = write_public_route(transaction, &record).await?;
    insert_audit_row(
        transaction,
        metadata,
        "content.publish",
        id,
        &before,
        &record,
        reason,
    )
    .await?;
    insert_outbox(
        transaction,
        "public.content.published",
        id,
        serde_json::json!({
            "entityId": id,
            "revision": revision,
            "locale": record.draft.locale,
            "canonicalPath": canonical_path,
            "dependencyCount": plan.dependencies.len(),
        }),
    )
    .await?;
    Ok(record)
}

async fn unpublish_attempt(
    transaction: &mut Transaction<'_, Postgres>,
    id: Uuid,
    expected: i64,
    expected_published_revision: i64,
    reason: &str,
    metadata: &MutationMetadata,
) -> Result<ContentRecordV2, AttemptError> {
    let before = lock_source(transaction, id).await?;
    ensure_expected_version(&before, expected, "unpublishing")?;
    match before.published_revision {
        Some(revision) if revision == expected_published_revision => {}
        Some(_) => {
            return Err(AttemptError::Api(ApiError::conflict(
                "The published revision changed; reload before unpublishing.",
            )))
        }
        None => {
            return Err(AttemptError::Api(ApiError::conflict(
                "The content is not currently published.",
            )))
        }
    }
    let blockers = dependencies::active_content_dependents(transaction, id).await?;
    if !blockers.is_empty() {
        return Err(AttemptError::Api(active_dependency_conflict(&blockers)));
    }
    let updated = sqlx::query(
        r#"UPDATE content_entries
           SET cms_published_revision=NULL,status='draft',cms_updated_by=$3
           WHERE id=$1 AND cms_published_revision=$2"#,
    )
    .bind(id)
    .bind(expected_published_revision)
    .bind(&metadata.actor)
    .execute(&mut **transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(AttemptError::Api(ApiError::conflict(
            "The published revision changed; reload before unpublishing.",
        )));
    }
    sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    let record = ContentRecordV2 {
        status: CmsPublicationStatusV2::Draft,
        published_revision: None,
        ..before.clone()
    };
    insert_audit_row(
        transaction,
        metadata,
        "content.unpublish",
        id,
        &before,
        &record,
        reason,
    )
    .await?;
    insert_outbox(
        transaction,
        "public.content.unpublished",
        id,
        serde_json::json!({
            "entityId": id,
            "revision": expected_published_revision,
            "locale": record.draft.locale,
        }),
    )
    .await?;
    Ok(record)
}
