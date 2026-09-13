use super::*;

impl AppState {
    /// Verify the database without copying runtime records into PlatformData.
    /// Business entities and settings are deliberately never snapshotted
    /// when PostgreSQL is configured: request-time SQL is the production source
    /// of truth, so independent API instances observe publishing immediately.
    pub async fn hydrate(&self) -> Result<(), ApiError> {
        if self.pool.is_none() {
            return Ok(());
        }
        self.check_persistence().await?;
        crate::services::cms_content::migrate_legacy_content(self).await?;
        Ok(())
    }

    // Kept temporarily as a private migration aid while the remaining legacy
    // handlers are converted to request-time SQL. It is never called by the
    // application and therefore cannot become a production business cache.
    #[allow(dead_code)]
    async fn hydrate_legacy_snapshot(&self) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        self.check_persistence().await?;
        let mut loaded = PlatformData {
            settings: self.platform_settings().await?,
            ..PlatformData::default()
        };

        for row in sqlx::query("SELECT payload FROM content_entries")
            .fetch_all(pool)
            .await?
        {
            let entry: ContentEntry = decode_payload(row.try_get("payload")?, "content")?;
            loaded.content.insert(entry.id, entry);
        }
        for row in sqlx::query("SELECT content_id, revision, payload FROM content_revisions")
            .fetch_all(pool)
            .await?
        {
            let content_id: Uuid = row.try_get("content_id")?;
            let revision: i64 = row.try_get("revision")?;
            let entry: ContentEntry = decode_payload(row.try_get("payload")?, "content revision")?;
            loaded
                .content_revisions
                .entry(content_id)
                .or_default()
                .insert(revision, entry);
        }
        for working in loaded.content.values() {
            let Some(revision) = working.published_revision else {
                continue;
            };
            if let Some(snapshot) = loaded
                .content_revisions
                .get(&working.id)
                .and_then(|revisions| revisions.get(&revision))
            {
                let mut published = snapshot.clone();
                published.status = crate::models::PublicationStatus::Published;
                published.published_revision = Some(revision);
                if published.is_placeholder {
                    published.seo.indexable = false;
                }
                loaded.published_content.insert(working.id, published);
            }
        }
        for row in sqlx::query(
            r#"SELECT id, connector_id, sync_run_id, source_record_id,
                      source_revision, checksum, source_payload, received_at
               FROM source_snapshots
               WHERE connector_id IS NOT NULL AND sync_run_id IS NOT NULL"#,
        )
        .fetch_all(pool)
        .await?
        {
            let snapshot = SourceSnapshot {
                id: row.try_get("id")?,
                connector_id: row.try_get("connector_id")?,
                sync_run_id: row.try_get("sync_run_id")?,
                source_record_id: row.try_get("source_record_id")?,
                source_revision: row.try_get("source_revision")?,
                checksum: row.try_get("checksum")?,
                source_payload: row.try_get("source_payload")?,
                received_at: row.try_get("received_at")?,
            };
            loaded.source_snapshots.insert(snapshot.id, snapshot);
        }
        for row in sqlx::query(
            r#"SELECT id, sync_run_id, source_snapshot_id, source_record_id,
                      validation_status, normalized_payload, validation_errors, created_at
               FROM staging_records"#,
        )
        .fetch_all(pool)
        .await?
        {
            let record = StagingRecord {
                id: row.try_get("id")?,
                sync_run_id: row.try_get("sync_run_id")?,
                source_snapshot_id: row.try_get("source_snapshot_id")?,
                source_record_id: row.try_get("source_record_id")?,
                validation_status: decode_enum(
                    row.try_get("validation_status")?,
                    "staging validation status",
                )?,
                normalized_payload: row.try_get("normalized_payload")?,
                validation_errors: decode_payload(
                    row.try_get("validation_errors")?,
                    "staging validation errors",
                )?,
                created_at: row.try_get("created_at")?,
            };
            loaded.staging_records.insert(record.id, record);
        }
        for row in sqlx::query("SELECT payload FROM products")
            .fetch_all(pool)
            .await?
        {
            let product: Product = decode_payload(row.try_get("payload")?, "product")?;
            loaded.products.insert(product.id, product);
        }
        for row in sqlx::query("SELECT product_id, revision, payload FROM product_revisions")
            .fetch_all(pool)
            .await?
        {
            let product_id: Uuid = row.try_get("product_id")?;
            let revision: i64 = row.try_get("revision")?;
            let product: Product = decode_payload(row.try_get("payload")?, "product revision")?;
            loaded
                .product_revisions
                .entry(product_id)
                .or_default()
                .insert(revision, product);
        }
        for row in sqlx::query(
            r#"SELECT product_id,locale,current_revision,published_revision,slug,title,
                      summary,content,seo_metadata,indexable,updated_at
               FROM product_presentation_working"#,
        )
        .fetch_all(pool)
        .await?
        {
            let product_id: Uuid = row.try_get("product_id")?;
            let locale: String = row.try_get("locale")?;
            let content: Value = row.try_get("content")?;
            let sort_order = content
                .get("sortOrder")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or_default();
            let related_content_ids = content
                .get("relatedContentIds")
                .cloned()
                .map(|value| decode_payload(value, "product related content ids"))
                .transpose()?
                .unwrap_or_default();
            loaded.product_presentations.insert(
                (product_id, locale.clone()),
                ProductPresentation {
                    locale,
                    slug: row.try_get("slug")?,
                    title: row.try_get("title")?,
                    summary: row.try_get("summary")?,
                    seo: decode_payload(row.try_get("seo_metadata")?, "product presentation SEO")?,
                    indexable: row.try_get("indexable")?,
                    sort_order,
                    related_content_ids,
                    revision: row.try_get("current_revision")?,
                    published_revision: row.try_get("published_revision")?,
                    updated_at: row.try_get("updated_at")?,
                },
            );
        }
        for working in loaded.products.values() {
            let Some(revision) = working.published_revision else {
                continue;
            };
            if let Some(snapshot) = loaded
                .product_revisions
                .get(&working.id)
                .and_then(|revisions| revisions.get(&revision))
            {
                let mut published = snapshot.clone();
                published.status = crate::models::PublicationStatus::Published;
                published.published_revision = Some(revision);
                loaded.published_products.insert(working.id, published);
            }
        }
        for row in sqlx::query("SELECT payload FROM sync_runs")
            .fetch_all(pool)
            .await?
        {
            let run: SyncRun = decode_payload(row.try_get("payload")?, "sync run")?;
            loaded.sync_runs.insert(run.id, run);
        }
        for row in sqlx::query("SELECT payload FROM rfq_submissions")
            .fetch_all(pool)
            .await?
        {
            let submission: RfqSubmission = decode_payload(row.try_get("payload")?, "RFQ")?;
            loaded.rfqs.insert(submission.id, submission);
        }
        for row in sqlx::query("SELECT payload FROM contact_requests")
            .fetch_all(pool)
            .await?
        {
            let contact: ContactRequest =
                decode_payload(row.try_get("payload")?, "contact request")?;
            loaded.contacts.insert(contact.id, contact);
        }
        for row in sqlx::query(
            "SELECT id, product_id, field_path, value, reason, created_at, expires_at FROM product_temporary_overrides WHERE resolved_at IS NULL",
        )
        .fetch_all(pool)
        .await?
        {
            let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
            let value = TemporaryOverride {
                id: row.try_get("id")?,
                product_id: row.try_get("product_id")?,
                field_path: row.try_get("field_path")?,
                value: row.try_get("value")?,
                reason: row.try_get("reason")?,
                created_at: row.try_get("created_at")?,
                expires_at,
                expired: expires_at <= Utc::now(),
            };
            loaded.temporary_overrides.insert(value.id, value);
        }
        for row in sqlx::query(
            "SELECT id, sync_run_id, product_id, source_record_id, field_diffs, resolved_at, resolution FROM sync_conflicts",
        )
        .fetch_all(pool)
        .await?
        {
            let conflict = SyncConflict {
                id: row.try_get("id")?,
                sync_run_id: row.try_get("sync_run_id")?,
                product_id: row.try_get("product_id")?,
                source_record_id: row.try_get("source_record_id")?,
                diffs: decode_payload(row.try_get("field_diffs")?, "sync conflict")?,
                resolved_at: row.try_get("resolved_at")?,
                resolution: row.try_get("resolution")?,
                revision: if row.try_get::<Option<DateTime<Utc>>, _>("resolved_at")?.is_some() {
                    2
                } else {
                    1
                },
            };
            loaded.conflicts.insert(conflict.id, conflict);
        }
        for row in sqlx::query(
            "SELECT id, kind, status, reason, result, created_at, updated_at FROM operation_runs",
        )
        .fetch_all(pool)
        .await?
        {
            let operation = BackgroundOperation {
                id: row.try_get("id")?,
                kind: decode_enum(row.try_get("kind")?, "operation kind")?,
                status: decode_enum(row.try_get("status")?, "operation status")?,
                reason: row.try_get("reason")?,
                result: row.try_get("result")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            loaded.operations.insert(operation.id, operation);
        }
        for row in sqlx::query(
            "SELECT id, actor, action, entity_type, entity_id, before_value, after_value, reason, request_id, occurred_at FROM audit_log ORDER BY occurred_at DESC LIMIT 1000",
        )
        .fetch_all(pool)
        .await?
        {
            loaded.audit_events.push(AuditEvent {
                id: row.try_get("id")?,
                actor: row.try_get("actor")?,
                action: row.try_get("action")?,
                entity_type: row.try_get("entity_type")?,
                entity_id: row.try_get("entity_id")?,
                before: row.try_get("before_value")?,
                after: row.try_get("after_value")?,
                reason: row.try_get("reason")?,
                request_id: row.try_get("request_id")?,
                occurred_at: row.try_get("occurred_at")?,
            });
        }

        *self.data.write().await = loaded;
        Ok(())
    }
}
