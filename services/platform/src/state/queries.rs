use super::*;

impl AppState {
    pub async fn list_working_content(&self) -> Result<Vec<ContentEntry>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.content.values().cloned().collect());
        };
        let rows = sqlx::query("SELECT payload FROM content_entries ORDER BY updated_at DESC,id")
            .fetch_all(pool)
            .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "content"))
            .collect()
    }

    pub async fn load_working_content(&self, id: Uuid) -> Result<Option<ContentEntry>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.content.get(&id).cloned());
        };
        sqlx::query_scalar::<_, Value>("SELECT payload FROM content_entries WHERE id=$1")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .map(|payload| decode_payload(payload, "content"))
            .transpose()
    }

    pub async fn content_identity_exists(
        &self,
        kind: crate::models::ContentKind,
        slug: &str,
        locale: &str,
        exclude_id: Option<Uuid>,
    ) -> Result<bool, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.content.values().any(|entry| {
                Some(entry.id) != exclude_id
                    && entry.kind == kind
                    && entry.slug == slug
                    && entry.locale == locale
            }));
        };
        Ok(sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(SELECT 1 FROM content_entries
               WHERE kind=$1 AND slug=$2 AND locale=$3
                 AND ($4::uuid IS NULL OR id<>$4))"#,
        )
        .bind(enum_label(kind))
        .bind(slug)
        .bind(locale)
        .bind(exclude_id)
        .fetch_one(pool)
        .await?)
    }

    pub async fn list_working_products(&self) -> Result<Vec<Product>, ApiError> {
        let Some(pool) = &self.pool else {
            let data = self.data.read().await;
            return Ok(data
                .products
                .values()
                .cloned()
                .map(|mut product| {
                    if let Some(presentation) = data
                        .product_presentations
                        .get(&(product.id, product.locale.clone()))
                    {
                        overlay_presentation(&mut product, presentation);
                    }
                    product
                })
                .collect());
        };
        let rows = sqlx::query(
            r#"SELECT product.payload,
                      presentation.current_revision AS presentation_revision,
                      presentation.locale AS presentation_locale,
                      presentation.slug AS presentation_slug,
                      presentation.title AS presentation_title,
                      presentation.summary AS presentation_summary,
                      presentation.content AS presentation_content,
                      presentation.seo_metadata AS presentation_seo,
                      presentation.indexable AS presentation_indexable
               FROM products product
               LEFT JOIN product_presentation_working presentation
                 ON presentation.product_id=product.id
                AND presentation.locale=product.locale
               ORDER BY product.stable_id,product.id"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let mut product = decode_payload(row.try_get("payload")?, "product")?;
                overlay_presentation_row(&mut product, &row)?;
                Ok(product)
            })
            .collect()
    }

    pub async fn load_working_product(&self, id: Uuid) -> Result<Option<Product>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.products.get(&id).cloned());
        };
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
        if let Some(pool) = &self.pool {
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
            return Ok(product);
        }
        if let Some(presentation) = self
            .data
            .read()
            .await
            .product_presentations
            .get(&(product.id, product.locale.clone()))
        {
            overlay_presentation(&mut product, presentation);
        }
        Ok(product)
    }

    pub async fn list_stored_rfqs(&self) -> Result<Vec<RfqSubmission>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.rfqs.values().cloned().collect());
        };
        let rows = sqlx::query("SELECT payload FROM rfq_submissions ORDER BY submitted_at DESC,id")
            .fetch_all(pool)
            .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "RFQ"))
            .collect()
    }

    pub async fn list_stored_contacts(&self) -> Result<Vec<ContactRequest>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.contacts.values().cloned().collect());
        };
        let rows =
            sqlx::query("SELECT payload FROM contact_requests ORDER BY submitted_at DESC,id")
                .fetch_all(pool)
                .await?;
        rows.into_iter()
            .map(|row| decode_payload(row.try_get("payload")?, "contact request"))
            .collect()
    }

    pub async fn list_stored_audit(&self) -> Result<Vec<AuditEvent>, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.audit_events.clone());
        };
        let rows = sqlx::query(
            r#"SELECT id,actor,action,entity_type,entity_id,before_value,after_value,
                      reason,request_id,occurred_at
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
                    request_id: row.try_get("request_id")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect()
    }
}
