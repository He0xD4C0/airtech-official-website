async fn update_product_presentation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<UpdateProductPresentation>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.productPresentation.update",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let detail: AdminProductDetail = replay.decode()?;
            let revision = detail
                .presentation
                .as_ref()
                .map(|presentation| presentation.revision)
                .unwrap_or_default();
            return Ok(entity_response(status, &detail, revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_product_presentation(&input)?;
    let before = load_admin_product_detail(&state, id).await?;
    validate_product_canonical(&input, before.product.family)?;
    let now = Utc::now();
    let after = if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        let product_row = sqlx::query(
            r#"SELECT current_revision,data_origin,family FROM products
               WHERE id=$1 FOR UPDATE"#,
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| ApiError::not_found("Product was not found."))?;
        if product_row.try_get::<i64, _>("current_revision")? != before.product.current_revision {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The Product Master facts changed; reload before saving presentation fields.",
            ));
        }
        let existing = sqlx::query(
            r#"SELECT current_revision,published_revision
               FROM product_presentation_working
               WHERE product_id=$1 AND locale=$2 FOR UPDATE"#,
        )
        .bind(id)
        .bind(&input.locale)
        .fetch_optional(&mut *transaction)
        .await?;
        let stored_revision = existing
            .as_ref()
            .map(|row| row.try_get::<i64, _>("current_revision"))
            .transpose()?
            .unwrap_or_default();
        if stored_revision != expected {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The product presentation changed; reload before saving.",
            ));
        }
        let product_family: String = product_row.try_get("family")?;
        let route_key = format!(
            "product-presentation:{}:{}:{}",
            product_family, input.locale, input.slug
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(route_key)
            .execute(&mut *transaction)
            .await?;
        let slug_owner = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT presentation.product_id
               FROM product_presentation_working presentation
               JOIN products other_product ON other_product.id=presentation.product_id
               WHERE presentation.locale=$1 AND presentation.slug=$2
                 AND presentation.product_id<>$3 AND other_product.family=$4"#,
        )
        .bind(&input.locale)
        .bind(&input.slug)
        .bind(id)
        .bind(&product_family)
        .fetch_optional(&mut *transaction)
        .await?;
        if slug_owner.is_some() {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "Another product in this family already uses this locale and slug.",
            ));
        }
        let revision = expected + 1;
        if !input.related_content_ids.is_empty() {
            let related_count = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM content_entries WHERE id=ANY($1)",
            )
            .bind(&input.related_content_ids)
            .fetch_one(&mut *transaction)
            .await?;
            if related_count != input.related_content_ids.len() as i64 {
                transaction.rollback().await?;
                return Err(ApiError::validation(BTreeMap::from([(
                    "relatedContentIds".into(),
                    vec!["Every related content id must exist.".into()],
                )])));
            }
        }
        let presentation_content = json!({
            "sortOrder": input.sort_order,
            "relatedContentIds": input.related_content_ids,
        });
        let presentation_seo = serde_json::to_value(&input.seo)
            .map_err(|_| ApiError::internal("Product presentation SEO serialization failed."))?;
        let product_data_origin: String = product_row.try_get("data_origin")?;
        let is_development_fixture = product_data_origin == "developmentFixture";
        let presentation_indexable = input.indexable && !is_development_fixture;
        let presentation_data_origin = if is_development_fixture {
            "developmentFixture"
        } else {
            "editorial"
        };
        let published_revision = existing
            .as_ref()
            .map(|row| row.try_get::<Option<i64>, _>("published_revision"))
            .transpose()?
            .flatten();
        if existing.is_some() {
            let updated = sqlx::query(
                r#"UPDATE product_presentation_working
                   SET current_revision=$3,slug=$4,title=$5,summary=$6,content=$7,
                       seo_metadata=$8,translation_state='draft',is_placeholder=$9,
                       indexable=$10,data_origin=$11,updated_by=$12,updated_at=$13
                   WHERE product_id=$1 AND locale=$2 AND current_revision=$14"#,
            )
            .bind(id)
            .bind(&input.locale)
            .bind(revision)
            .bind(&input.slug)
            .bind(&input.title)
            .bind(&input.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(is_development_fixture)
            .bind(presentation_indexable)
            .bind(presentation_data_origin)
            .bind(&actor_name)
            .bind(now)
            .bind(expected)
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The product presentation changed; reload before saving.",
                ));
            }
        } else {
            sqlx::query(
                r#"INSERT INTO product_presentation_working
                   (product_id,locale,current_revision,published_revision,slug,title,summary,
                    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                    updated_by,updated_at)
                   VALUES ($1,$2,$3,NULL,$4,$5,$6,$7,$8,'draft',$9,$10,$11,$12,$13)"#,
            )
            .bind(id)
            .bind(&input.locale)
            .bind(revision)
            .bind(&input.slug)
            .bind(&input.title)
            .bind(&input.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(is_development_fixture)
            .bind(presentation_indexable)
            .bind(presentation_data_origin)
            .bind(&actor_name)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            r#"INSERT INTO product_presentation_revisions
               (product_id,locale,revision,source_product_revision,slug,title,summary,
                content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                created_by,created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'draft',$10,$11,$12,$13,$14)"#,
        )
        .bind(id)
        .bind(&input.locale)
        .bind(revision)
        .bind(before.product.current_revision)
        .bind(&input.slug)
        .bind(&input.title)
        .bind(&input.summary)
        .bind(&presentation_content)
        .bind(&presentation_seo)
        .bind(is_development_fixture)
        .bind(presentation_indexable)
        .bind(presentation_data_origin)
        .bind(&actor_name)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let presentation = ProductPresentation {
            locale: input.locale.clone(),
            slug: input.slug.clone(),
            title: input.title.clone(),
            summary: input.summary.clone(),
            seo: input.seo.clone(),
            indexable: presentation_indexable,
            sort_order: input.sort_order,
            related_content_ids: input.related_content_ids.clone(),
            revision,
            published_revision,
            updated_at: now,
        };
        let mut after = before.clone();
        overlay_product_presentation(&mut after.product, &presentation);
        after.presentation = Some(presentation);
        let audit = mutation_audit_event(
            &headers,
            "product.presentation.update",
            "productPresentation",
            Some(id),
            Some(json!(&before)),
            Some(json!(&after)),
            Some(input.reason.clone()),
        );
        insert_audit_event_in_transaction(&mut transaction, &audit).await?;
        let staged = idempotency
            .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
            .await?;
        transaction.commit().await?;
        staged.finish().await?;
        after
    } else {
        let mut data = state.data.write().await;
        let current_presentation = data
            .product_presentations
            .get(&(id, input.locale.clone()))
            .cloned()
            .or_else(|| before.presentation.clone());
        let stored_revision = current_presentation
            .as_ref()
            .map(|presentation| presentation.revision)
            .unwrap_or_default();
        if stored_revision != expected {
            return Err(ApiError::conflict(
                "The product presentation changed; reload before saving.",
            ));
        }
        if data
            .product_presentations
            .iter()
            .any(|((product_id, locale), presentation)| {
                *product_id != id
                    && locale == &input.locale
                    && presentation.slug == input.slug
                    && data
                        .products
                        .get(product_id)
                        .is_some_and(|other| other.family == before.product.family)
            })
        {
            return Err(ApiError::conflict(
                "Another product in this family already uses this locale and slug.",
            ));
        }
        if input
            .related_content_ids
            .iter()
            .any(|content_id| !data.content.contains_key(content_id))
        {
            return Err(ApiError::validation(BTreeMap::from([(
                "relatedContentIds".into(),
                vec!["Every related content id must exist.".into()],
            )])));
        }
        let presentation = ProductPresentation {
            locale: input.locale.clone(),
            slug: input.slug.clone(),
            title: input.title.clone(),
            summary: input.summary.clone(),
            seo: input.seo.clone(),
            indexable: input.indexable
                && !matches!(
                    &before.source_kind,
                    crate::models::DataClass::DevelopmentFixture
                ),
            sort_order: input.sort_order,
            related_content_ids: input.related_content_ids.clone(),
            revision: expected + 1,
            published_revision: current_presentation
                .and_then(|presentation| presentation.published_revision),
            updated_at: now,
        };
        data.product_presentations
            .insert((id, input.locale.clone()), presentation.clone());
        let mut after = before.clone();
        overlay_product_presentation(&mut after.product, &presentation);
        after.presentation = Some(presentation);
        drop(data);
        let audit = mutation_audit_event(
            &headers,
            "product.presentation.update",
            "productPresentation",
            Some(id),
            Some(json!(&before)),
            Some(json!(&after)),
            Some(input.reason),
        );
        persist_memory_audit_if_needed(&state, audit).await?;
        idempotency.complete(&state, &after, StatusCode::OK).await?;
        after
    };
    let revision = after
        .presentation
        .as_ref()
        .map(|presentation| presentation.revision)
        .unwrap_or_default();
    Ok(entity_response(StatusCode::OK, &after, revision))
}
