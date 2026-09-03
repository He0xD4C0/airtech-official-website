// ---- Persistence helpers ------------------------------------------------------------------
async fn load_admin_product_detail(
    state: &AppState,
    id: Uuid,
) -> Result<AdminProductDetail, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT payload,data_origin,current_revision,product_import_run_id,stable_id
               FROM products WHERE id=$1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Product was not found."))?;
        let mut product: crate::models::Product = decode_json(row.try_get("payload")?, "product")?;
        let origin: String = row.try_get("data_origin")?;
        let presentation_row = sqlx::query(
            r#"SELECT locale,slug,title,summary,content,seo_metadata,indexable,
                      current_revision,published_revision,updated_at
               FROM product_presentation_working
               WHERE product_id=$1
               ORDER BY CASE WHEN locale=$2 THEN 0 ELSE 1 END,locale LIMIT 1"#,
        )
        .bind(id)
        .bind(&product.locale)
        .fetch_optional(pool)
        .await?;
        let presentation = presentation_row
            .map(|presentation| {
                let content: Value = presentation.try_get("content")?;
                let sort_order = content
                    .get("sortOrder")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or_default();
                let related_content_ids = content
                    .get("relatedContentIds")
                    .cloned()
                    .map(|value| decode_json(value, "product related content ids"))
                    .transpose()?
                    .unwrap_or_default();
                Ok::<ProductPresentation, ApiError>(ProductPresentation {
                    locale: presentation.try_get("locale")?,
                    slug: presentation.try_get("slug")?,
                    title: presentation.try_get("title")?,
                    summary: presentation.try_get("summary")?,
                    seo: decode_json(
                        presentation.try_get("seo_metadata")?,
                        "product presentation SEO",
                    )?,
                    indexable: presentation.try_get("indexable")?,
                    sort_order,
                    related_content_ids,
                    revision: presentation.try_get("current_revision")?,
                    published_revision: presentation.try_get("published_revision")?,
                    updated_at: presentation.try_get("updated_at")?,
                })
            })
            .transpose()?;
        if let Some(presentation) = &presentation {
            overlay_product_presentation(&mut product, presentation);
        }
        let import_run_id: Option<Uuid> = row.try_get("product_import_run_id")?;
        let missing_assets = if let Some(import_run_id) = import_run_id {
            sqlx::query(
                r#"SELECT source_record_id,asset_type,source_reference
                   FROM product_import_missing_assets
                   WHERE import_run_id=$1 AND source_record_id=$2 AND resolution_status='missing'
                   ORDER BY created_at,id"#,
            )
            .bind(import_run_id)
            .bind(row.try_get::<String, _>("stable_id")?)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|missing| {
                Ok(crate::models::MissingAssetReference {
                    stable_id: missing.try_get("source_record_id")?,
                    asset_type: missing.try_get("asset_type")?,
                    source_reference: missing.try_get("source_reference")?,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
        } else {
            Vec::new()
        };
        return Ok(AdminProductDetail {
            product,
            source_kind: match origin.as_str() {
                "verifiedCsv" => DataClass::VerifiedCsv,
                "developmentFixture" => DataClass::DevelopmentFixture,
                "feishu" => DataClass::Feishu,
                _ => DataClass::Editorial,
            },
            missing_assets,
            presentation,
        });
    }
    let data = state.data.read().await;
    let mut product = data
        .products
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Product was not found."))?;
    let presentation = data
        .product_presentations
        .get(&(id, product.locale.clone()))
        .cloned()
        .unwrap_or_else(|| ProductPresentation {
            locale: product.locale.clone(),
            slug: product.slug.clone(),
            title: product.title.clone(),
            summary: product.summary.clone(),
            seo: product.seo.clone(),
            indexable: product.indexable,
            sort_order: product.sort_order,
            related_content_ids: product.related_content_ids.clone(),
            revision: 1,
            published_revision: product.published_revision.map(|_| 1),
            updated_at: product.updated_at,
        });
    overlay_product_presentation(&mut product, &presentation);
    Ok(AdminProductDetail {
        presentation: Some(presentation),
        product,
        source_kind: DataClass::Feishu,
        missing_assets: Vec::new(),
    })
}

fn overlay_product_presentation(
    product: &mut crate::models::Product,
    presentation: &ProductPresentation,
) {
    product.slug = presentation.slug.clone();
    product.locale = presentation.locale.clone();
    product.title = presentation.title.clone();
    product.summary = presentation.summary.clone();
    product.seo = presentation.seo.clone();
    product.indexable = presentation.indexable;
    product.sort_order = presentation.sort_order;
    product.related_content_ids = presentation.related_content_ids.clone();
}
