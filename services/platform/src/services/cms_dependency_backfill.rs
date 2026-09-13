//! Operational gate for dependency snapshots on published CMS V2 revisions.

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use super::cms_publication_dependencies::{self as dependencies, CMS_DEPENDENCY_EXTRACTOR_VERSION};

const BACKFILL_ACTOR: &str = "airtekctl:cms-dependencies-backfill";
const PUBLICATION_LOCK_KEY: &str = "airtek.cms.publication.v2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyBackfillMode {
    Check,
    Apply,
}

impl DependencyBackfillMode {
    fn label(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Apply => "apply",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyBackfillItem {
    pub content_id: Uuid,
    pub content_revision: i64,
    pub status: &'static str,
    pub dependency_count: usize,
    pub blocking_issues: Vec<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyBackfillReport {
    pub mode: &'static str,
    pub published_revisions: usize,
    pub already_complete: usize,
    pub ready: usize,
    pub written_complete: usize,
    pub written_blocked: usize,
    pub items: Vec<DependencyBackfillItem>,
}

impl DependencyBackfillReport {
    pub fn can_release(&self) -> bool {
        self.items
            .iter()
            .all(|item| matches!(item.status, "complete" | "alreadyComplete" | "ready"))
    }
}

pub async fn run(
    pool: &PgPool,
    mode: DependencyBackfillMode,
) -> Result<DependencyBackfillReport, String> {
    let candidates: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"SELECT id,cms_published_revision
           FROM content_entries
           WHERE cms_published_revision IS NOT NULL
           ORDER BY id"#,
    )
    .fetch_all(pool)
    .await
    .map_err(database_error)?;
    let mut report = DependencyBackfillReport {
        mode: mode.label(),
        published_revisions: candidates.len(),
        already_complete: 0,
        ready: 0,
        written_complete: 0,
        written_blocked: 0,
        items: Vec::new(),
    };
    for (content_id, revision) in candidates {
        let Some(outcome) = inspect_one(pool, mode, content_id, revision).await? else {
            // It was unpublished or republished after the candidate list was read.
            continue;
        };
        match outcome.status {
            "alreadyComplete" => report.already_complete += 1,
            "ready" => report.ready += 1,
            "complete" => report.written_complete += 1,
            "blocked" if mode == DependencyBackfillMode::Apply => report.written_blocked += 1,
            _ => {}
        }
        report.items.push(outcome);
    }
    report.published_revisions = report.items.len();
    Ok(report)
}

async fn inspect_one(
    pool: &PgPool,
    mode: DependencyBackfillMode,
    content_id: Uuid,
    revision: i64,
) -> Result<Option<DependencyBackfillItem>, String> {
    let mut transaction = serializable_transaction(pool).await?;
    let row = sqlx::query(
        r#"SELECT revision.document,set.extractor_version,set.extraction_status,
                  cms_publication_dependency_snapshot_complete($1,$2) AS snapshot_complete
           FROM content_entries entry
           JOIN content_revisions revision
             ON revision.content_id=entry.id
            AND revision.revision=entry.cms_published_revision
           LEFT JOIN cms_publication_dependency_sets set
             ON set.content_id=revision.content_id AND set.content_revision=revision.revision
           WHERE entry.id=$1 AND entry.cms_published_revision=$2
           FOR UPDATE OF entry"#,
    )
    .bind(content_id)
    .bind(revision)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(database_error)?;
    let Some(row) = row else {
        transaction.rollback().await.map_err(database_error)?;
        return Ok(None);
    };
    let extractor: Option<String> = row.try_get("extractor_version").map_err(database_error)?;
    let extraction_status: Option<String> =
        row.try_get("extraction_status").map_err(database_error)?;
    let snapshot_complete: bool = row.try_get("snapshot_complete").map_err(database_error)?;
    if snapshot_complete
        && extractor.as_deref() == Some(CMS_DEPENDENCY_EXTRACTOR_VERSION)
        && extraction_status.as_deref() == Some("complete")
    {
        let dependency_count: i64 = sqlx::query_scalar(
            r#"SELECT count(*) FROM cms_publication_dependencies
               WHERE source_content_id=$1 AND source_revision=$2"#,
        )
        .bind(content_id)
        .bind(revision)
        .fetch_one(&mut *transaction)
        .await
        .map_err(database_error)?;
        transaction.rollback().await.map_err(database_error)?;
        return Ok(Some(DependencyBackfillItem {
            content_id,
            content_revision: revision,
            status: "alreadyComplete",
            dependency_count: dependency_count as usize,
            blocking_issues: Vec::new(),
        }));
    }
    if extraction_status.as_deref() == Some("complete") {
        transaction.rollback().await.map_err(database_error)?;
        return Ok(Some(immutable_complete_blocker(
            content_id,
            revision,
            extractor.as_deref(),
        )));
    }

    let document: Option<Value> = row.try_get("document").map_err(database_error)?;
    let document = document.unwrap_or(Value::Null);
    let extracted = dependencies::extract_document(&document);
    dependencies::lock_targets(&mut transaction, &extracted.lock_targets)
        .await
        .map_err(database_error)?;
    let plan = dependencies::validate_extracted(&mut transaction, extracted)
        .await
        .map_err(database_error)?;
    let blocking_issues = plan
        .blocking_issues
        .iter()
        .map(|issue| {
            serde_json::to_value(issue).unwrap_or_else(|_| {
                json!({
                    "code": "serializationFailed",
                    "detail": "A dependency issue could not be serialized."
                })
            })
        })
        .collect::<Vec<_>>();
    let dependency_count = plan.dependencies.len();
    if mode == DependencyBackfillMode::Check {
        transaction.rollback().await.map_err(database_error)?;
        return Ok(Some(DependencyBackfillItem {
            content_id,
            content_revision: revision,
            status: if plan.is_complete() {
                "ready"
            } else {
                "blocked"
            },
            dependency_count,
            blocking_issues,
        }));
    }

    if extraction_status.as_deref() == Some("blocked") {
        sqlx::query(
            "DELETE FROM cms_publication_dependency_sets WHERE content_id=$1 AND content_revision=$2",
        )
        .bind(content_id)
        .bind(revision)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    }
    dependencies::insert_snapshot(
        &mut transaction,
        content_id,
        revision,
        BACKFILL_ACTOR,
        &plan,
    )
    .await
    .map_err(database_error)?;
    transaction.commit().await.map_err(database_error)?;
    Ok(Some(DependencyBackfillItem {
        content_id,
        content_revision: revision,
        status: if plan.is_complete() {
            "complete"
        } else {
            "blocked"
        },
        dependency_count,
        blocking_issues,
    }))
}

async fn serializable_transaction(pool: &PgPool) -> Result<Transaction<'_, Postgres>, String> {
    let mut transaction = pool.begin().await.map_err(database_error)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(PUBLICATION_LOCK_KEY)
        .execute(&mut *transaction)
        .await
        .map_err(database_error)?;
    Ok(transaction)
}

fn immutable_complete_blocker(
    content_id: Uuid,
    revision: i64,
    extractor: Option<&str>,
) -> DependencyBackfillItem {
    DependencyBackfillItem {
        content_id,
        content_revision: revision,
        status: "blocked",
        dependency_count: 0,
        blocking_issues: vec![json!({
            "code": "immutableCompleteSnapshotMismatch",
            "path": "/",
            "failedGate": "documentShape",
            "targetId": null,
            "detail": format!(
                "An immutable complete snapshot is inconsistent or uses extractor {:?}; expected {}.",
                extractor, CMS_DEPENDENCY_EXTRACTOR_VERSION
            )
        })],
    }
}

fn database_error(error: sqlx::Error) -> String {
    format!("CMS dependency backfill database operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_gate_accepts_only_non_blocking_item_states() {
        let mut report = DependencyBackfillReport {
            mode: "check",
            published_revisions: 1,
            already_complete: 0,
            ready: 1,
            written_complete: 0,
            written_blocked: 0,
            items: vec![DependencyBackfillItem {
                content_id: Uuid::nil(),
                content_revision: 1,
                status: "ready",
                dependency_count: 0,
                blocking_issues: vec![],
            }],
        };
        assert!(report.can_release());
        report.items[0].status = "blocked";
        assert!(!report.can_release());
    }
}
