use super::*;

impl AppState {
    pub async fn persist_rfq(&self, value: &RfqSubmission) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO rfq_submissions
               (id, reference, journey, status, source_path, locale, submitted_at,
                retention_until, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(value.id)
        .bind(&value.reference)
        .bind(enum_label(value.request.journey))
        .bind(&value.status)
        .bind(&value.request.source_path)
        .bind(&value.request.locale)
        .bind(value.submitted_at)
        .bind(value.retention_until)
        .bind(
            serde_json::to_value(value)
                .map_err(|_| ApiError::internal("RFQ serialization failed."))?,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn persist_contact(&self, value: &ContactRequest) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO contact_requests
               (id, reference, topic, status, source_path, locale, submitted_at,
                retention_until, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(value.id)
        .bind(&value.reference)
        .bind(&value.request.topic)
        .bind(&value.status)
        .bind(&value.request.source_path)
        .bind(&value.request.locale)
        .bind(value.submitted_at)
        .bind(value.retention_until)
        .bind(
            serde_json::to_value(value)
                .map_err(|_| ApiError::internal("Contact serialization failed."))?,
        )
        .execute(pool)
        .await?;
        Ok(())
    }
}
