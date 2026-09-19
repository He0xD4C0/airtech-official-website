use super::*;

impl AppState {
    pub(super) async fn postgres_product_publication_issues(
        &self,
        product: &Product,
    ) -> Result<Vec<ValidationIssue>, ApiError> {
        let mut connection = self.pool.acquire().await?;
        self.postgres_product_publication_issues_on(&mut connection, product)
            .await
    }

    pub(super) async fn postgres_product_publication_issues_on(
        &self,
        connection: &mut PgConnection,
        product: &Product,
    ) -> Result<Vec<ValidationIssue>, ApiError> {
        let mut issues = Vec::new();
        let working_row = sqlx::query(
            r#"SELECT product.payload AS working_payload,
                      product.source_snapshot_id AS working_source_snapshot_id,
                      product.source_revision AS working_source_revision,
                      product.data_origin AS working_data_origin,
                      product.product_import_run_id AS working_import_run_id,
                      product.current_revision AS working_revision,
                      revision.payload AS immutable_payload,
                      revision.source_snapshot_id AS immutable_source_snapshot_id,
                      revision.data_origin AS immutable_data_origin,
                      revision.product_import_run_id AS immutable_import_run_id
               FROM products AS product
               LEFT JOIN product_revisions AS revision
                 ON revision.product_id=product.id
                AND revision.revision=product.current_revision
               WHERE product.id=$1"#,
        )
        .bind(product.id)
        .fetch_optional(&mut *connection)
        .await?;
        let mut data_origin = String::new();
        let mut import_run_id = None;
        if let Some(row) = working_row {
            let working_payload: Value = row.try_get("working_payload")?;
            let stored_working: Product = decode_payload(working_payload, "working product")?;
            let immutable_payload: Option<Value> = row.try_get("immutable_payload")?;
            let immutable_source_snapshot_id: Option<Uuid> =
                row.try_get("immutable_source_snapshot_id")?;
            let working_source_snapshot_id: Option<Uuid> =
                row.try_get("working_source_snapshot_id")?;
            data_origin = row.try_get("working_data_origin")?;
            import_run_id = row.try_get("working_import_run_id")?;
            let immutable_data_origin: String = row.try_get("immutable_data_origin")?;
            let immutable_import_run_id: Option<Uuid> = row.try_get("immutable_import_run_id")?;
            let expected_snapshot =
                (!product.source_snapshot_id.is_nil()).then_some(product.source_snapshot_id);
            let immutable_matches = immutable_payload
                .and_then(|payload| decode_payload::<Product>(payload, "product revision").ok())
                .is_some_and(|revision| {
                    immutable_source_snapshot_id == expected_snapshot
                        && immutable_revision_payload(&revision).ok()
                            == immutable_revision_payload(product).ok()
                });
            if row.try_get::<i64, _>("working_revision")? != product.current_revision
                || working_source_snapshot_id != expected_snapshot
                || row.try_get::<String, _>("working_source_revision")?.trim()
                    != product.source_revision.trim()
                || immutable_data_origin != data_origin
                || immutable_import_run_id != import_run_id
                || immutable_revision_payload(&stored_working).ok()
                    != immutable_revision_payload(product).ok()
                || !immutable_matches
            {
                issues.push(workflow_error(
                    "currentRevision",
                    "immutableRevisionMismatch",
                    "The stored working Product and current immutable revision must match the requested source revision.",
                ));
            }
            if data_origin == "verifiedCsv" {
                issues.extend(validate_verified_csv_product_master(&stored_working));
            } else {
                issues.extend(validate_product_master(&stored_working));
            }
        } else {
            issues.push(workflow_error(
                "currentRevision",
                "immutableRevisionMissing",
                "The stored working Product and current immutable revision are required.",
            ));
        }

        let override_rows = sqlx::query(
            r#"SELECT field_path, expires_at <= now() AS expired
               FROM product_temporary_overrides
               WHERE product_id=$1 AND resolved_at IS NULL"#,
        )
        .bind(product.id)
        .fetch_all(&mut *connection)
        .await?;
        let mut active_override_paths = Vec::new();
        let mut expired_override = false;
        for row in override_rows {
            if row.try_get::<bool, _>("expired")? {
                expired_override = true;
            } else {
                active_override_paths.push(row.try_get::<String, _>("field_path")?);
            }
        }
        if expired_override {
            issues.push(workflow_error(
                "temporaryOverrides",
                "expiredOverride",
                "An unresolved expired temporary override blocks publication.",
            ));
        }

        if data_origin == "verifiedCsv" {
            let evidence = sqlx::query(
                r#"SELECT run.environment,run.status,run.source_checksum,run.mapping_version,
                          run.records_received,run.records_valid,
                          record.validation_status,record.normalized_payload
                   FROM product_import_runs run
                   LEFT JOIN product_import_normalized_records record
                     ON record.import_run_id=run.id AND record.source_record_id=$2
                   WHERE run.id=$1 AND run.data_origin='verifiedCsv'"#,
            )
            .bind(import_run_id)
            .bind(&product.stable_id)
            .fetch_optional(&mut *connection)
            .await?;
            match evidence {
                Some(row) => {
                    let status: String = row.try_get("status")?;
                    let environment: String = row.try_get("environment")?;
                    let checksum: String = row.try_get("source_checksum")?;
                    let mapping_version: String = row.try_get("mapping_version")?;
                    let records_received: i64 = row.try_get("records_received")?;
                    let records_valid: i64 = row.try_get("records_valid")?;
                    let validation_status: Option<String> = row.try_get("validation_status")?;
                    let normalized: Option<Value> = row.try_get("normalized_payload")?;
                    if status != "completed"
                        || mapping_version.trim().is_empty()
                        || validation_status.as_deref() != Some("valid")
                        || product.source_revision != format!("csv:{checksum}")
                    {
                        issues.push(workflow_error(
                            "productImportRunId",
                            "verifiedCsvEvidenceInvalid",
                            "Verified CSV publication requires a completed matching import run and valid normalized record.",
                        ));
                    }
                    if environment != self.environment_label()
                        || verify_product_master_authority(
                            self.environment_label(),
                            self.config.approved_product_master.as_ref(),
                            &checksum,
                            &mapping_version,
                            records_valid,
                            records_received.saturating_sub(records_valid),
                        )
                        .is_err()
                    {
                        issues.push(workflow_error(
                            "productImportRunId",
                            "verifiedCsvAuthorityMismatch",
                            "The Product Master import does not match this deployment's registered source, mapping, environment, and reviewed row counts.",
                        ));
                    }
                    if let Some(normalized) = normalized {
                        issues.extend(validate_accepted_staging_payload(product, &normalized));
                        issues.extend(validate_source_owned_alignment(
                            product,
                            &normalized,
                            &active_override_paths,
                        ));
                    } else {
                        issues.push(workflow_error(
                            "productImportRunId",
                            "verifiedCsvRecordMissing",
                            "The normalized verified CSV record is missing.",
                        ));
                    }
                }
                None => issues.push(workflow_error(
                    "productImportRunId",
                    "verifiedCsvImportMissing",
                    "The verified CSV import run is missing.",
                )),
            }
            return Ok(issues);
        }

        let row = sqlx::query(
            r#"SELECT snapshot.connector_id,
                      connector.connector_type,
                      snapshot.sync_run_id AS snapshot_sync_run_id,
                      snapshot.source_record_id AS snapshot_source_record_id,
                      snapshot.source_revision AS snapshot_source_revision,
                      snapshot.checksum AS snapshot_checksum,
                      staging.source_record_id AS staging_source_record_id,
                      staging.validation_status,
                      staging.normalized_payload,
                      staging.validation_errors,
                      run.source AS sync_source,
                      run.dry_run AS sync_dry_run,
                      run.status AS sync_status,
                      run.mapping_version AS sync_mapping_version
               FROM source_snapshots AS snapshot
               LEFT JOIN source_connectors AS connector ON connector.id=snapshot.connector_id
               LEFT JOIN staging_records AS staging
                 ON staging.source_snapshot_id=snapshot.id
                AND staging.sync_run_id=snapshot.sync_run_id
                AND staging.source_record_id=snapshot.source_record_id
               LEFT JOIN sync_runs AS run ON run.id=staging.sync_run_id
               WHERE snapshot.id=$1
               ORDER BY staging.created_at DESC
               LIMIT 1"#,
        )
        .bind(product.source_snapshot_id)
        .fetch_optional(&mut *connection)
        .await?;

        if let Some(row) = row {
            let connector_id: Option<Uuid> = row.try_get("connector_id")?;
            let connector_type: Option<String> = row.try_get("connector_type")?;
            let snapshot_sync_run_id: Option<Uuid> = row.try_get("snapshot_sync_run_id")?;
            let snapshot_source_record_id: String = row.try_get("snapshot_source_record_id")?;
            let snapshot_source_revision: String = row.try_get("snapshot_source_revision")?;
            let snapshot_checksum: String = row.try_get("snapshot_checksum")?;
            if connector_id.is_none()
                || connector_type.as_deref() != Some("feishu")
                || snapshot_sync_run_id.is_none()
                || snapshot_checksum.trim().is_empty()
            {
                issues.push(workflow_error(
                    "sourceSnapshotId",
                    "invalidSourceSnapshot",
                    "The source snapshot is not bound to a Feishu connector, sync run and checksum.",
                ));
            }
            if snapshot_source_revision.trim() != product.source_revision.trim() {
                issues.push(workflow_error(
                    "sourceRevision",
                    "sourceRevisionMismatch",
                    "The working Product source revision does not match its source snapshot.",
                ));
            }

            let staging_source_record_id: Option<String> =
                row.try_get("staging_source_record_id")?;
            let validation_status: Option<String> = row.try_get("validation_status")?;
            let validation_errors: Option<Value> = row.try_get("validation_errors")?;
            if staging_source_record_id.as_deref() != Some(snapshot_source_record_id.as_str())
                || validation_status.as_deref() != Some("valid")
                || !validation_errors
                    .as_ref()
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
            {
                issues.push(workflow_error(
                    "staging.validationStatus",
                    "stagingNotAccepted",
                    "The exact source snapshot has not passed staging validation without errors.",
                ));
            }
            match row.try_get::<Option<Value>, _>("normalized_payload")? {
                Some(payload) if !payload.is_null() => {
                    issues.extend(validate_accepted_staging_payload(product, &payload));
                    issues.extend(validate_source_owned_alignment(
                        product,
                        &payload,
                        &active_override_paths,
                    ));
                }
                _ => issues.push(workflow_error(
                    "staging.normalizedPayload",
                    "normalizedPayloadRequired",
                    "Accepted staging evidence requires a normalized Product Master payload.",
                )),
            }
            let sync_source: Option<String> = row.try_get("sync_source")?;
            let sync_dry_run: Option<bool> = row.try_get("sync_dry_run")?;
            let sync_status: Option<String> = row.try_get("sync_status")?;
            let sync_mapping_version: Option<String> = row.try_get("sync_mapping_version")?;
            if sync_source.as_deref() != Some("feishu")
                || sync_dry_run != Some(false)
                || sync_mapping_version
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(|value| value.is_empty())
                || !matches!(sync_status.as_deref(), Some("readyToPublish" | "completed"))
            {
                issues.push(workflow_error(
                    "staging.syncRunId",
                    "syncRunNotAccepted",
                    "Publishing requires a non-dry-run Feishu sync in readyToPublish or completed state.",
                ));
            }
        } else {
            issues.push(workflow_error(
                "sourceSnapshotId",
                "sourceSnapshotNotFound",
                "The referenced Feishu source snapshot does not exist.",
            ));
            issues.push(workflow_error(
                "staging.validationStatus",
                "stagingRecordNotFound",
                "No accepted staging record exists for the exact source snapshot.",
            ));
        }

        Ok(issues)
    }
}
