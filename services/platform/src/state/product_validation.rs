use super::*;

impl AppState {
    pub async fn product_publication_issues(
        &self,
        product: &Product,
    ) -> Result<Vec<ValidationIssue>, ApiError> {
        if self.pool.is_some() {
            self.postgres_product_publication_issues(product).await
        } else {
            let mut issues = validate_product_master(product);
            issues.extend(self.in_memory_product_publication_issues(product).await);
            Ok(issues)
        }
    }

    pub async fn assert_product_publishable(&self, product: &Product) -> Result<(), ApiError> {
        let issues = self.product_publication_issues(product).await?;
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ApiError::validation(issues_as_errors(issues)))
        }
    }

    /// Presentation publication has its own revision clock. A Product Master
    /// fact revision can remain current while an editor has a newer website
    /// draft that still needs to be projected publicly.
    pub async fn product_presentation_is_published(
        &self,
        product_id: Uuid,
        locale: &str,
    ) -> Result<bool, ApiError> {
        if let Some(pool) = &self.pool {
            return Ok(sqlx::query_scalar::<_, bool>(
                r#"SELECT published_revision=current_revision
                   FROM product_presentation_working
                   WHERE product_id=$1 AND locale=$2"#,
            )
            .bind(product_id)
            .bind(locale)
            .fetch_optional(pool)
            .await?
            .unwrap_or(false));
        }
        Ok(self
            .data
            .read()
            .await
            .product_presentations
            .get(&(product_id, locale.to_owned()))
            .is_some_and(|presentation| {
                presentation.published_revision == Some(presentation.revision)
            }))
    }

    pub(super) async fn in_memory_product_publication_issues(
        &self,
        product: &Product,
    ) -> Vec<ValidationIssue> {
        let data = self.data.read().await;
        let mut issues = Vec::new();
        let immutable_revision_matches = data
            .product_revisions
            .get(&product.id)
            .and_then(|revisions| revisions.get(&product.current_revision))
            .is_some_and(|revision| {
                revision.source_snapshot_id == product.source_snapshot_id
                    && immutable_revision_payload(revision).ok()
                        == immutable_revision_payload(product).ok()
            });
        if !immutable_revision_matches {
            issues.push(workflow_error(
                "currentRevision",
                "immutableRevisionMismatch",
                "The current immutable Product revision is missing or differs from the working record.",
            ));
        }
        let now = Utc::now();
        let active_override_paths: Vec<_> = data
            .temporary_overrides
            .values()
            .filter(|value| value.product_id == product.id && value.expires_at > now)
            .map(|value| value.field_path.clone())
            .collect();
        let snapshot = data.source_snapshots.get(&product.source_snapshot_id);
        let source_record_id = if let Some(snapshot) = snapshot {
            if snapshot.connector_id.is_nil()
                || snapshot.sync_run_id.is_nil()
                || snapshot.checksum.trim().is_empty()
            {
                issues.push(workflow_error(
                    "sourceSnapshotId",
                    "invalidSourceSnapshot",
                    "The source snapshot is not bound to a Feishu connector, sync run and checksum.",
                ));
            }
            if snapshot.source_revision.trim() != product.source_revision.trim() {
                issues.push(workflow_error(
                    "sourceRevision",
                    "sourceRevisionMismatch",
                    "The working Product source revision does not match its source snapshot.",
                ));
            }
            Some(snapshot.source_record_id.as_str())
        } else {
            issues.push(workflow_error(
                "sourceSnapshotId",
                "sourceSnapshotNotFound",
                "The referenced Feishu source snapshot does not exist.",
            ));
            None
        };

        let staging = snapshot.and_then(|snapshot| {
            data.staging_records
                .values()
                .filter(|record| {
                    record.source_snapshot_id == snapshot.id
                        && record.sync_run_id == snapshot.sync_run_id
                        && record.source_record_id == snapshot.source_record_id
                })
                .max_by_key(|record| record.created_at)
        });
        if let Some(staging) = staging {
            if staging.validation_status != StagingValidationStatus::Valid
                || !staging.validation_errors.is_empty()
            {
                issues.push(workflow_error(
                    "staging.validationStatus",
                    "stagingNotAccepted",
                    "The exact source snapshot has not passed staging validation without errors.",
                ));
            }
            match &staging.normalized_payload {
                Some(payload) if !payload.is_null() => {
                    issues.extend(validate_accepted_staging_payload(product, payload));
                    issues.extend(validate_source_owned_alignment(
                        product,
                        payload,
                        &active_override_paths,
                    ));
                }
                _ => issues.push(workflow_error(
                    "staging.normalizedPayload",
                    "normalizedPayloadRequired",
                    "Accepted staging evidence requires a normalized Product Master payload.",
                )),
            }
            match data.sync_runs.get(&staging.sync_run_id) {
                Some(run)
                    if run.source == "feishu"
                        && !run.dry_run
                        && matches!(
                            run.status,
                            SyncRunStatus::ReadyToPublish | SyncRunStatus::Completed
                        ) => {}
                _ => issues.push(workflow_error(
                    "staging.syncRunId",
                    "syncRunNotAccepted",
                    "Publishing requires a non-dry-run Feishu sync in readyToPublish or completed state.",
                )),
            }
        } else {
            issues.push(workflow_error(
                "staging.validationStatus",
                "stagingRecordNotFound",
                "No accepted staging record exists for the exact source snapshot.",
            ));
        }

        if data.conflicts.values().any(|conflict| {
            conflict.resolved_at.is_none()
                && (conflict.product_id == Some(product.id)
                    || source_record_id.is_some_and(|source_record_id| {
                        conflict.source_record_id == source_record_id
                    }))
        }) {
            issues.push(workflow_error(
                "conflicts",
                "openConflict",
                "All open three-way conflicts related to this Product must be resolved.",
            ));
        }
        if data
            .temporary_overrides
            .values()
            .any(|value| value.product_id == product.id && value.expires_at <= now)
        {
            issues.push(workflow_error(
                "temporaryOverrides",
                "expiredOverride",
                "An unresolved expired temporary override blocks publication.",
            ));
        }
        issues
    }
}
