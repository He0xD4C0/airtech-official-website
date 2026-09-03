use super::*;

impl AppState {
    pub async fn persist_operation(&self, operation: &BackgroundOperation) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let mut transaction = pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO operation_runs
               (id, kind, status, reason, result, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7)
               ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status,
                 result=EXCLUDED.result,updated_at=EXCLUDED.updated_at"#,
        )
        .bind(operation.id)
        .bind(enum_label(operation.kind))
        .bind(enum_label(operation.status))
        .bind(&operation.reason)
        .bind(&operation.result)
        .bind(operation.created_at)
        .bind(operation.updated_at)
        .execute(&mut *transaction)
        .await?;
        // Product Master uploads are validated and promoted before the 202 is
        // returned in the current phase-one implementation. Never enqueue a
        // second orphan job containing an operation-only payload; a durable
        // worker import will use a private encrypted staging reference instead.
        if operation.status == crate::models::OperationStatus::Queued
            && operation.kind != crate::models::OperationKind::ProductImport
        {
            sqlx::query(
                r#"INSERT INTO jobs (id, job_type, status, payload, available_at, created_at, updated_at)
                   VALUES ($1,$2,'queued',$3,$4,$4,$4) ON CONFLICT (id) DO NOTHING"#,
            )
            .bind(operation.id)
            .bind(enum_label(operation.kind))
            .bind(serde_json::to_value(operation).map_err(|_| {
                ApiError::internal("Operation serialization failed.")
            })?)
            .bind(operation.created_at)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn list_operations(&self) -> Result<Vec<BackgroundOperation>, ApiError> {
        if let Some(pool) = &self.pool {
            let rows = sqlx::query(
                "SELECT id, kind, status, reason, result, created_at, updated_at FROM operation_runs ORDER BY created_at DESC",
            )
            .fetch_all(pool)
            .await?;
            return rows
                .into_iter()
                .map(|row| {
                    Ok(BackgroundOperation {
                        id: row.try_get("id")?,
                        kind: decode_enum(row.try_get("kind")?, "operation kind")?,
                        status: decode_enum(row.try_get("status")?, "operation status")?,
                        reason: row.try_get("reason")?,
                        result: row.try_get("result")?,
                        created_at: row.try_get("created_at")?,
                        updated_at: row.try_get("updated_at")?,
                    })
                })
                .collect();
        }
        let mut values: Vec<_> = self
            .data
            .read()
            .await
            .operations
            .values()
            .cloned()
            .collect();
        values.sort_by_key(|operation| std::cmp::Reverse(operation.created_at));
        Ok(values)
    }

    pub async fn get_operation(
        &self,
        operation_id: Uuid,
    ) -> Result<Option<BackgroundOperation>, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                "SELECT id, kind, status, reason, result, created_at, updated_at FROM operation_runs WHERE id=$1",
            )
            .bind(operation_id)
            .fetch_optional(pool)
            .await?;
            return row
                .map(|row| {
                    Ok(BackgroundOperation {
                        id: row.try_get("id")?,
                        kind: decode_enum(row.try_get("kind")?, "operation kind")?,
                        status: decode_enum(row.try_get("status")?, "operation status")?,
                        reason: row.try_get("reason")?,
                        result: row.try_get("result")?,
                        created_at: row.try_get("created_at")?,
                        updated_at: row.try_get("updated_at")?,
                    })
                })
                .transpose();
        }
        Ok(self
            .data
            .read()
            .await
            .operations
            .get(&operation_id)
            .cloned())
    }

    pub async fn persist_audit(&self, event: AuditEvent) -> Result<(), ApiError> {
        if let Some(pool) = &self.pool {
            sqlx::query(
                r#"INSERT INTO audit_log
                   (id, actor, action, entity_type, entity_id, before_value, after_value,
                    reason, request_id, occurred_at)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
            )
            .bind(event.id)
            .bind(&event.actor)
            .bind(&event.action)
            .bind(&event.entity_type)
            .bind(event.entity_id)
            .bind(&event.before)
            .bind(&event.after)
            .bind(&event.reason)
            .bind(event.request_id)
            .bind(event.occurred_at)
            .execute(pool)
            .await?;
        } else {
            self.data.write().await.audit_events.push(event);
        }
        Ok(())
    }
}
