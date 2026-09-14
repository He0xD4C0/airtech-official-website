use super::*;

impl AppState {
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
