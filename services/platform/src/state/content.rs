use super::*;

impl AppState {
    pub async fn persist_content(&self, entry: &ContentEntry, actor: &str) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content serialization failed."))?;
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                r#"INSERT INTO content_entries
               (id, kind, slug, locale, title, status, is_placeholder, current_revision,
                published_revision, scheduled_for, payload, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
               ON CONFLICT (id) DO UPDATE SET kind=EXCLUDED.kind, slug=EXCLUDED.slug,
               locale=EXCLUDED.locale, title=EXCLUDED.title, status=EXCLUDED.status,
               is_placeholder=EXCLUDED.is_placeholder,
               current_revision=EXCLUDED.current_revision,
               published_revision=EXCLUDED.published_revision,
               scheduled_for=EXCLUDED.scheduled_for, payload=EXCLUDED.payload,
               updated_at=EXCLUDED.updated_at"#,
            )
            .bind(entry.id)
            .bind(enum_label(entry.kind))
            .bind(&entry.slug)
            .bind(&entry.locale)
            .bind(&entry.title)
            .bind(enum_label(entry.status))
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(entry.published_revision)
            .bind(entry.scheduled_for)
            .bind(&payload)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            // Creation is an explicit snapshot boundary. Later autosaves only
            // update the working row; preview, publish, and rollback materialize
            // any additional immutable snapshots.
            sqlx::query(
                r#"INSERT INTO content_revisions
                   (content_id,revision,payload,created_by,created_at)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (content_id,revision) DO NOTHING"#,
            )
            .bind(entry.id)
            .bind(entry.current_revision)
            .bind(&payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        }
        if self.pool.is_none() {
            self.data
                .write()
                .await
                .content_revisions
                .entry(entry.id)
                .or_default()
                .entry(entry.current_revision)
                .or_insert_with(|| entry.clone());
        }
        Ok(())
    }

    /// Loads one immutable content snapshot. Preview callers must never fall
    /// back to the working record or the published projection when this exact
    /// revision does not exist.
    pub async fn load_content_revision(
        &self,
        content_id: Uuid,
        revision: i64,
    ) -> Result<Option<ContentEntry>, ApiError> {
        if let Some(pool) = &self.pool {
            let payload = sqlx::query_scalar::<_, Value>(
                "SELECT payload FROM content_revisions WHERE content_id=$1 AND revision=$2",
            )
            .bind(content_id)
            .bind(revision)
            .fetch_optional(pool)
            .await?;
            return payload
                .map(|payload| decode_payload(payload, "content preview revision"))
                .transpose();
        }

        Ok(self
            .data
            .read()
            .await
            .content_revisions
            .get(&content_id)
            .and_then(|revisions| revisions.get(&revision))
            .cloned())
    }

    /// Materialize an explicit preview snapshot of the current working
    /// document. Autosave itself does not create immutable history; preview,
    /// publish and rollback are the explicit snapshot boundaries.
    pub async fn snapshot_working_content(
        &self,
        entry: &ContentEntry,
        actor: &str,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content snapshot serialization failed."))?;
        if let Some(pool) = &self.pool {
            sqlx::query(
                r#"INSERT INTO content_revisions
                   (content_id,revision,payload,created_by,created_at)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (content_id,revision) DO NOTHING"#,
            )
            .bind(entry.id)
            .bind(entry.current_revision)
            .bind(payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(pool)
            .await?;
        }
        if self.pool.is_none() {
            self.data
                .write()
                .await
                .content_revisions
                .entry(entry.id)
                .or_default()
                .entry(entry.current_revision)
                .or_insert_with(|| entry.clone());
        }
        Ok(())
    }

    pub async fn persist_content_update(
        &self,
        entry: &ContentEntry,
        _actor: &str,
        expected_revision: i64,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content serialization failed."))?;
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            let result = sqlx::query(
                r#"UPDATE content_entries SET kind=$2, slug=$3, locale=$4, title=$5,
                   status=$6, is_placeholder=$7, current_revision=$8,
                   published_revision=$9, scheduled_for=$10, payload=$11, updated_at=$12,
                   data_origin=CASE
                     WHEN data_origin='developmentFixture' AND NOT $7 THEN 'editorial'
                     ELSE data_origin
                   END
                   WHERE id=$1 AND current_revision=$13"#,
            )
            .bind(entry.id)
            .bind(enum_label(entry.kind))
            .bind(&entry.slug)
            .bind(&entry.locale)
            .bind(&entry.title)
            .bind(enum_label(entry.status))
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(entry.published_revision)
            .bind(entry.scheduled_for)
            .bind(&payload)
            .bind(entry.updated_at)
            .bind(expected_revision)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The content entry changed; reload before saving.",
                ));
            }
            transaction.commit().await?;
        }
        Ok(())
    }

    /// Atomically moves the public pointer to an immutable revision and emits
    /// the invalidation/indexing event consumed by the worker.
    pub async fn publish_content_projection(
        &self,
        entry: &ContentEntry,
        actor: &str,
        event: &str,
        expected_revision: i64,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(entry)
            .map_err(|_| ApiError::internal("Content serialization failed."))?;
        let outbox_id = Uuid::new_v4();
        let outbox_payload = serde_json::json!({
            "entityId": entry.id,
            "revision": entry.current_revision,
            "locale": entry.locale,
            "kind": enum_label(entry.kind),
            "event": event
        });
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            let result = sqlx::query(
                r#"UPDATE content_entries
                   SET kind=$2, slug=$3, locale=$4, title=$5, status='published',
                       is_placeholder=$6, current_revision=$7, published_revision=$7,
                       scheduled_for=NULL, payload=$8, updated_at=$9,
                       data_origin=CASE
                         WHEN data_origin='developmentFixture' AND NOT $6 THEN 'editorial'
                         ELSE data_origin
                       END
                   WHERE id=$1 AND current_revision=$10"#,
            )
            .bind(entry.id)
            .bind(enum_label(entry.kind))
            .bind(&entry.slug)
            .bind(&entry.locale)
            .bind(&entry.title)
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(&payload)
            .bind(entry.updated_at)
            .bind(expected_revision)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The content entry changed; reload before publishing.",
                ));
            }
            // A content entity owns at most one canonical route. Replacing or
            // removing the canonical path must retire every older locale/path
            // in the same publication transaction.
            sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
                .bind(entry.id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                r#"INSERT INTO content_revisions
                   (content_id, revision, payload, created_by, created_at)
                   VALUES ($1,$2,$3,$4,$5)
                   ON CONFLICT (content_id, revision) DO NOTHING"#,
            )
            .bind(entry.id)
            .bind(entry.current_revision)
            .bind(&payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            if let Some(canonical_path) = entry.seo.canonical_path.as_deref() {
                let route_owner = sqlx::query_scalar::<_, Uuid>(
                    "SELECT entity_id FROM public_routes WHERE canonical_path=$1",
                )
                .bind(canonical_path)
                .fetch_optional(&mut *transaction)
                .await?;
                if route_owner.is_some_and(|owner| owner != entry.id) {
                    transaction.rollback().await?;
                    return Err(ApiError::conflict(
                        "Another published entity already owns this canonical path.",
                    ));
                }
                sqlx::query(
                    r#"INSERT INTO public_routes
                       (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
                       VALUES ($1,'content',$2,$3,$4,$5,$6)
                       ON CONFLICT (entity_type,entity_id,locale) DO UPDATE SET
                         canonical_path=EXCLUDED.canonical_path,
                         indexable=EXCLUDED.indexable,updated_at=EXCLUDED.updated_at"#,
                )
                .bind(Uuid::new_v4())
                .bind(entry.id)
                .bind(&entry.locale)
                .bind(canonical_path)
                .bind(entry.seo.indexable && !entry.is_placeholder)
                .bind(entry.updated_at)
                .execute(&mut *transaction)
                .await?;
            }
            sqlx::query(
                r#"INSERT INTO outbox_events
                   (id, topic, aggregate_type, aggregate_id, payload)
                   VALUES ($1,'public.content.published','content',$2,$3)
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(outbox_id)
            .bind(entry.id)
            .bind(&outbox_payload)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        }
        if self.pool.is_none() {
            let mut data = self.data.write().await;
            data.content_revisions
                .entry(entry.id)
                .or_default()
                .entry(entry.current_revision)
                .or_insert_with(|| entry.clone());
            data.published_content.insert(entry.id, entry.clone());
            if !data.outbox_events.iter().any(|value| {
                value.get("topic").and_then(Value::as_str) == Some("public.content.published")
                    && value.get("aggregateId") == Some(&serde_json::json!(entry.id))
                    && value.pointer("/payload/revision")
                        == Some(&serde_json::json!(entry.current_revision))
            }) {
                data.outbox_events.push(serde_json::json!({
                    "id": outbox_id,
                    "topic": "public.content.published",
                    "aggregateId": entry.id,
                    "payload": outbox_payload
                }));
            }
        }
        Ok(())
    }
}
