use super::*;

impl AppState {
    pub async fn publish_product_projection(
        &self,
        working: &Product,
        product: &Product,
    ) -> Result<(), ApiError> {
        let payload = serde_json::to_value(product)
            .map_err(|_| ApiError::internal("Product serialization failed."))?;
        let outbox_id = Uuid::new_v4();
        {
            let pool = &self.pool;
            let mut transaction = pool.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
                .execute(&mut *transaction)
                .await?;
            sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
                .bind(working.id)
                .fetch_optional(&mut *transaction)
                .await?;
            let issues = self
                .postgres_product_publication_issues_on(&mut transaction, working)
                .await?;
            if !issues.is_empty() {
                transaction.rollback().await?;
                return Err(ApiError::validation(issues_as_errors(issues)));
            }
            let presentation = sqlx::query(
                r#"SELECT current_revision,slug,title,summary,content,
                          seo_metadata,is_placeholder,indexable,data_origin,updated_by
                   FROM product_presentation_working
                   WHERE product_id=$1 AND locale=$2
                   FOR UPDATE"#,
            )
            .bind(product.id)
            .bind(&product.locale)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| {
                ApiError::validation(std::collections::BTreeMap::from([(
                    "presentation".into(),
                    vec![
                        "A localized presentation working revision is required before publication."
                            .into(),
                    ],
                )]))
            })?;
            let presentation_revision: i64 = presentation.try_get("current_revision")?;
            let slug: String = presentation.try_get("slug")?;
            let seo: Value = presentation.try_get("seo_metadata")?;
            let canonical_path = seo
                .get("canonicalPath")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(canonical_path) = canonical_path.as_deref() {
                let family = match product.family {
                    airtek_domain::models::ProductFamily::Centrifugal => "centrifugal",
                    airtek_domain::models::ProductFamily::Axial => "axial",
                    airtek_domain::models::ProductFamily::CrossFlow => "cross-flow",
                    airtek_domain::models::ProductFamily::InlineDuct => "inline-duct",
                    airtek_domain::models::ProductFamily::Motors => "motors",
                };
                let expected_path = format!("/en/products/{family}/{slug}");
                if canonical_path != expected_path
                    || canonical_path.contains(['?', '#', '\\', '\r', '\n', '\0'])
                {
                    transaction.rollback().await?;
                    return Err(ApiError::validation(std::collections::BTreeMap::from([(
                        "seo.canonicalPath".into(),
                        vec![format!("Must exactly match {expected_path}.")],
                    )])));
                }
                let route_owner = sqlx::query_scalar::<_, Uuid>(
                    "SELECT entity_id FROM public_routes WHERE canonical_path=$1",
                )
                .bind(canonical_path)
                .fetch_optional(&mut *transaction)
                .await?;
                if route_owner.is_some_and(|owner| owner != product.id) {
                    transaction.rollback().await?;
                    return Err(ApiError::conflict(
                        "Another published entity already owns this product canonical path.",
                    ));
                }
            }
            let is_placeholder: bool = presentation.try_get("is_placeholder")?;
            let presentation_indexable: bool = presentation.try_get("indexable")?;
            let result = sqlx::query(
                r#"UPDATE products
                   SET status='published', published_revision=current_revision,
                       indexable=$2, payload=$3, updated_at=$4
                   WHERE id=$1 AND current_revision=$5"#,
            )
            .bind(product.id)
            .bind(presentation_indexable && !is_placeholder)
            .bind(&payload)
            .bind(product.updated_at)
            .bind(product.current_revision)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The product changed; reload before publishing.",
                ));
            }
            sqlx::query("DELETE FROM public_routes WHERE entity_type='product' AND entity_id=$1")
                .bind(product.id)
                .execute(&mut *transaction)
                .await?;
            if let Some(canonical_path) = canonical_path.as_deref() {
                sqlx::query(
                    r#"INSERT INTO public_routes
                       (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
                       VALUES ($1,'product',$2,$3,$4,$5,$6)"#,
                )
                .bind(Uuid::new_v4())
                .bind(product.id)
                .bind(&product.locale)
                .bind(canonical_path)
                .bind(presentation_indexable && !is_placeholder)
                .bind(product.updated_at)
                .execute(&mut *transaction)
                .await?;
            }
            sqlx::query(
                r#"INSERT INTO product_localizations
                   (product_id,product_revision,locale,slug,title,summary,content,seo_metadata,
                    translation_state,is_placeholder,indexable,data_origin,updated_by,updated_at)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'verified',$9,$10,$11,$12,$13)
                   ON CONFLICT (product_id,product_revision,locale) DO UPDATE SET
                     slug=EXCLUDED.slug,title=EXCLUDED.title,summary=EXCLUDED.summary,
                     content=EXCLUDED.content,seo_metadata=EXCLUDED.seo_metadata,
                     translation_state='verified',is_placeholder=EXCLUDED.is_placeholder,
                     indexable=EXCLUDED.indexable,data_origin=EXCLUDED.data_origin,
                     updated_by=EXCLUDED.updated_by,updated_at=EXCLUDED.updated_at"#,
            )
            .bind(product.id)
            .bind(product.current_revision)
            .bind(&product.locale)
            .bind(&slug)
            .bind(presentation.try_get::<String, _>("title")?)
            .bind(presentation.try_get::<Option<String>, _>("summary")?)
            .bind(presentation.try_get::<Value, _>("content")?)
            .bind(&seo)
            .bind(is_placeholder)
            .bind(presentation_indexable && !is_placeholder)
            .bind(presentation.try_get::<String, _>("data_origin")?)
            .bind(presentation.try_get::<String, _>("updated_by")?)
            .bind(product.updated_at)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"UPDATE product_presentation_working
                   SET published_revision=current_revision,translation_state='verified'
                   WHERE product_id=$1 AND locale=$2 AND current_revision=$3"#,
            )
            .bind(product.id)
            .bind(&product.locale)
            .bind(presentation_revision)
            .execute(&mut *transaction)
            .await?;
            let outbox_payload = serde_json::json!({
                "entityId": product.id,
                "factRevision": product.current_revision,
                "presentationRevision": presentation_revision,
                "locale": product.locale
            });
            sqlx::query(
                r#"INSERT INTO outbox_events
                   (id, topic, aggregate_type, aggregate_id, payload)
                   VALUES ($1,'public.product.published','product',$2,$3)
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(outbox_id)
            .bind(product.id)
            .bind(&outbox_payload)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        }
        Ok(())
    }
}
