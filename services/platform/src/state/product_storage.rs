use super::*;

impl AppState {
    pub async fn persist_override(&self, value: &TemporaryOverride) -> Result<(), ApiError> {
        let pool = &self.pool;
        sqlx::query(
            r#"INSERT INTO product_temporary_overrides
               (id, product_id, field_path, value, reason, created_at, expires_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
        )
        .bind(value.id)
        .bind(value.product_id)
        .bind(&value.field_path)
        .bind(&value.value)
        .bind(&value.reason)
        .bind(value.created_at)
        .bind(value.expires_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn persist_product(&self, product: &Product) -> Result<(), ApiError> {
        {
            let pool = &self.pool;
            let mut transaction = pool.begin().await?;
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("product:stable-id:{}", product.stable_id))
                .execute(&mut *transaction)
                .await?;
            let existing = sqlx::query(
                r#"SELECT id,current_revision,published_revision,data_origin,
                          source_snapshot_id,source_revision
                   FROM products WHERE stable_id=$1 FOR UPDATE"#,
            )
            .bind(&product.stable_id)
            .fetch_optional(&mut *transaction)
            .await?;
            let mut accepted = product.clone();
            // Feishu acceptance creates a working facts revision only. It can
            // never make that revision public or indexable without the
            // separate publication transaction and its evidence checks.
            accepted.status = crate::models::PublicationStatus::Draft;
            accepted.indexable = false;
            let mut insert_product = true;
            if let Some(existing) = existing {
                let data_origin: String = existing.try_get("data_origin")?;
                if data_origin == "developmentFixture" {
                    transaction.rollback().await?;
                    return Err(ApiError::conflict(
                        "A development fixture cannot be taken over as a Feishu Product.",
                    ));
                }
                let existing_id: Uuid = existing.try_get("id")?;
                let existing_revision: i64 = existing.try_get("current_revision")?;
                let existing_snapshot: Option<Uuid> = existing.try_get("source_snapshot_id")?;
                let existing_source_revision: String = existing.try_get("source_revision")?;
                let same_source = data_origin == "feishu"
                    && existing_snapshot == Some(product.source_snapshot_id)
                    && existing_source_revision == product.source_revision;
                accepted.id = existing_id;
                accepted.current_revision = if same_source {
                    existing_revision
                } else {
                    existing_revision + 1
                };
                accepted.published_revision = existing.try_get("published_revision")?;
                insert_product = false;
            }
            let mut payload = serde_json::to_value(&accepted)
                .map_err(|_| ApiError::internal("Product serialization failed."))?;
            if let Some(object) = payload.as_object_mut() {
                object.insert("dataOrigin".into(), Value::String("feishu".into()));
            }
            if insert_product {
                sqlx::query(
                    r#"INSERT INTO products
                       (id,stable_id,model,slug,locale,family,source_snapshot_id,
                        source_revision,status,current_revision,published_revision,indexable,
                        payload,updated_at,data_origin,product_import_run_id)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,
                               'feishu',NULL)"#,
                )
                .bind(accepted.id)
                .bind(&accepted.stable_id)
                .bind(&accepted.model)
                .bind(&accepted.slug)
                .bind(&accepted.locale)
                .bind(enum_label(accepted.family))
                .bind(accepted.source_snapshot_id)
                .bind(&accepted.source_revision)
                .bind(enum_label(accepted.status))
                .bind(accepted.current_revision)
                .bind(accepted.published_revision)
                .bind(accepted.indexable)
                .bind(&payload)
                .bind(accepted.updated_at)
                .execute(&mut *transaction)
                .await?;
            } else {
                sqlx::query(
                    r#"UPDATE products SET model=$2,slug=$3,locale=$4,family=$5,
                              source_snapshot_id=$6,source_revision=$7,status=$8,
                              current_revision=$9,published_revision=$10,indexable=$11,
                              payload=$12,updated_at=$13,data_origin='feishu',
                              product_import_run_id=NULL
                       WHERE id=$1"#,
                )
                .bind(accepted.id)
                .bind(&accepted.model)
                .bind(&accepted.slug)
                .bind(&accepted.locale)
                .bind(enum_label(accepted.family))
                .bind(accepted.source_snapshot_id)
                .bind(&accepted.source_revision)
                .bind(enum_label(accepted.status))
                .bind(accepted.current_revision)
                .bind(accepted.published_revision)
                .bind(accepted.indexable)
                .bind(&payload)
                .bind(accepted.updated_at)
                .execute(&mut *transaction)
                .await?;
            }
            sqlx::query(
                r#"INSERT INTO product_revisions
                   (product_id,revision,source_snapshot_id,payload,created_at,data_origin,
                    product_import_run_id)
                   VALUES ($1,$2,$3,$4,$5,'feishu',NULL)
                   ON CONFLICT (product_id,revision) DO NOTHING"#,
            )
            .bind(accepted.id)
            .bind(accepted.current_revision)
            .bind(accepted.source_snapshot_id)
            .bind(&payload)
            .bind(accepted.updated_at)
            .execute(&mut *transaction)
            .await?;
            let stored_revision = sqlx::query(
                r#"SELECT source_snapshot_id, payload
                   FROM product_revisions
                   WHERE product_id=$1 AND revision=$2"#,
            )
            .bind(accepted.id)
            .bind(accepted.current_revision)
            .fetch_one(&mut *transaction)
            .await?;
            let stored_source_snapshot_id: Uuid = stored_revision.try_get("source_snapshot_id")?;
            let stored_payload: Value = stored_revision.try_get("payload")?;
            let stored_product: Product = decode_payload(stored_payload, "product revision")?;
            if stored_source_snapshot_id != accepted.source_snapshot_id
                || immutable_revision_payload(&stored_product).ok()
                    != immutable_revision_payload(&accepted).ok()
            {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "An immutable Product revision already exists with different data.",
                ));
            }
            let staging_facts = sqlx::query_scalar::<_, Value>(
                r#"SELECT normalized_payload FROM staging_records
                   WHERE source_snapshot_id=$1 AND source_record_id=$2
                     AND validation_status='valid'
                   ORDER BY created_at DESC LIMIT 1"#,
            )
            .bind(accepted.source_snapshot_id)
            .bind(&accepted.stable_id)
            .fetch_optional(&mut *transaction)
            .await?;
            let mut facts_payload = payload.clone();
            if let Some(staging) = staging_facts.and_then(|value| value.as_object().cloned()) {
                if let Some(target) = facts_payload.as_object_mut() {
                    for key in [
                        "specifications",
                        "operatingConditions",
                        "performanceCurves",
                        "assets",
                    ] {
                        if let Some(value) = staging.get(key) {
                            target.insert(key.into(), value.clone());
                        }
                    }
                }
            }
            project_product_facts(
                &mut transaction,
                accepted.id,
                accepted.current_revision,
                &facts_payload,
                &format!("feishu:{}:{}", accepted.source_revision, accepted.stable_id),
            )
            .await?;
            let presentation_content = serde_json::json!({
                "sortOrder": accepted.sort_order,
                "relatedContentIds": accepted.related_content_ids,
            });
            let presentation_seo = serde_json::to_value(&accepted.seo).map_err(|_| {
                ApiError::internal("Product presentation SEO serialization failed.")
            })?;
            sqlx::query(
                r#"INSERT INTO product_presentation_working
                   (product_id,locale,current_revision,published_revision,slug,title,summary,
                    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                    updated_by,updated_at)
                   VALUES ($1,$2,1,NULL,$3,$4,$5,$6,$7,'draft',false,$8,'editorial',$9,$10)
                   ON CONFLICT (product_id,locale) DO NOTHING"#,
            )
            .bind(accepted.id)
            .bind(&accepted.locale)
            .bind(&accepted.slug)
            .bind(&accepted.title)
            .bind(&accepted.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(accepted.indexable)
            .bind("productMasterIngestion")
            .bind(accepted.updated_at)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"INSERT INTO product_presentation_revisions
                   (product_id,locale,revision,source_product_revision,slug,title,summary,
                    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                    created_by,created_at)
                   VALUES ($1,$2,1,$3,$4,$5,$6,$7,$8,'draft',false,$9,'editorial',$10,$11)
                   ON CONFLICT (product_id,locale,revision) DO NOTHING"#,
            )
            .bind(accepted.id)
            .bind(&accepted.locale)
            .bind(accepted.current_revision)
            .bind(&accepted.slug)
            .bind(&accepted.title)
            .bind(&accepted.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(accepted.indexable)
            .bind("productMasterIngestion")
            .bind(accepted.updated_at)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        }
        Ok(())
    }
}
