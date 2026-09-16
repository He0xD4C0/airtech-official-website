use super::*;

impl AppState {
    pub async fn load_working_product(&self, id: Uuid) -> Result<Option<Product>, ApiError> {
        let pool = &self.pool;
        sqlx::query_scalar::<_, Value>("SELECT payload FROM products WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(|payload| decode_payload(payload, "product"))
            .transpose()
    }

    /// Combine Product Master facts with the current portal-owned presentation
    /// for an Admin response without ever writing the combined DTO back into
    /// the immutable fact payload.
    pub async fn present_product(&self, mut product: Product) -> Result<Product, ApiError> {
        let pool = &self.pool;
        let row = sqlx::query(
            r#"SELECT current_revision AS presentation_revision,
                          locale AS presentation_locale,slug AS presentation_slug,
                          title AS presentation_title,summary AS presentation_summary,
                          content AS presentation_content,seo_metadata AS presentation_seo,
                          indexable AS presentation_indexable
                   FROM product_presentation_working
                   WHERE product_id=$1 AND locale=$2"#,
        )
        .bind(product.id)
        .bind(&product.locale)
        .fetch_optional(pool)
        .await?;
        if let Some(row) = row {
            overlay_presentation_row(&mut product, &row)?;
        }
        Ok(product)
    }

    pub async fn list_stored_rfqs(&self) -> Result<Vec<RfqSubmission>, ApiError> {
        let pool = &self.pool;
        let rows = sqlx::query("SELECT payload FROM rfq_submissions ORDER BY submitted_at DESC,id")
            .fetch_all(pool)
            .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "RFQ"))
            .collect()
    }

    pub async fn list_stored_contacts(&self) -> Result<Vec<ContactRequest>, ApiError> {
        let pool = &self.pool;
        let rows =
            sqlx::query("SELECT payload FROM contact_requests ORDER BY submitted_at DESC,id")
                .fetch_all(pool)
                .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "contact request"))
            .collect()
    }

    pub async fn list_stored_audit(&self) -> Result<Vec<AuditEvent>, ApiError> {
        let pool = &self.pool;
        let rows = sqlx::query(
            r#"SELECT id,actor,action,entity_type,entity_id,before_value,after_value,
                      reason,current_version,request_id,occurred_at
               FROM audit_log ORDER BY occurred_at DESC,id"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AuditEvent {
                    id: row.try_get("id")?,
                    actor: row.try_get("actor")?,
                    action: row.try_get("action")?,
                    entity_type: row.try_get("entity_type")?,
                    entity_id: row.try_get("entity_id")?,
                    before: row.try_get("before_value")?,
                    after: row.try_get("after_value")?,
                    reason: row.try_get("reason")?,
                    current_version: row.try_get("current_version")?,
                    request_id: row.try_get("request_id")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect()
    }
}
