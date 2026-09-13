#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
}

async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductListQuery>,
) -> Result<Json<CursorPage<Product>>, ApiError> {
    let needle = parse_query_text(query.q)?.map(|value| value.to_lowercase());
    let mut values = state.list_working_products().await?;
    let input_records = values.len();
    let started = std::time::Instant::now();
    if let Some(needle) = needle {
        values.retain(|product| product_matches_query(product, &needle));
    }
    values.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    let page = paginate_by_id(
        "admin.products",
        values,
        CursorQuery {
            cursor: query.cursor,
            limit: query.limit,
        },
        |product| product.id,
    )?;
    crate::services::list_filter_observability::observe_in_memory_filter(
        "admin.products",
        input_records,
        started.elapsed(),
    );
    Ok(Json(page))
}

fn product_matches_query(product: &Product, needle: &str) -> bool {
    product.stable_id.to_lowercase().contains(needle)
        || product
            .model
            .as_deref()
            .is_some_and(|model| model.to_lowercase().contains(needle))
        || product.title.to_lowercase().contains(needle)
        || product_family_matches_query(product.family, needle)
}

fn product_family_matches_query(family: crate::models::ProductFamily, needle: &str) -> bool {
    let (contract_name, display_name) = match family {
        crate::models::ProductFamily::Centrifugal => ("centrifugal", "centrifugal fans"),
        crate::models::ProductFamily::Axial => ("axial", "axial fans"),
        crate::models::ProductFamily::CrossFlow => ("crossflow", "cross-flow fans"),
        crate::models::ProductFamily::InlineDuct => ("inlineduct", "inline duct fans"),
        crate::models::ProductFamily::Motors => ("motors", "motors"),
    };
    contract_name.contains(needle) || display_name.contains(needle)
}

#[cfg(test)]
mod product_query_tests {
    use super::*;

    fn product() -> Product {
        Product {
            id: Uuid::nil(),
            stable_id: "ATK-AF-001".into(),
            model: Some("E2E-MODEL-001".into()),
            slug: "atk-af-001".into(),
            locale: "en".into(),
            family: crate::models::ProductFamily::Axial,
            subtype: None,
            motor_technology: None,
            title: "Verified axial fan".into(),
            summary: None,
            seo: Default::default(),
            sort_order: 0,
            related_content_ids: Vec::new(),
            specifications: Vec::new(),
            performance_curves: Vec::new(),
            source_snapshot_id: Uuid::nil(),
            source_revision: "test".into(),
            current_revision: 1,
            published_revision: None,
            status: PublicationStatus::Draft,
            indexable: false,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn product_query_matches_id_model_title_and_family_case_insensitively() {
        let product = product();
        assert!(product_matches_query(&product, "atk-af"));
        assert!(product_matches_query(&product, "e2e-model"));
        assert!(product_matches_query(&product, "verified axial"));
        assert!(product_matches_query(&product, "axial fans"));
        assert!(!product_matches_query(&product, "centrifugal"));

        let mut cross_flow = product;
        cross_flow.family = crate::models::ProductFamily::CrossFlow;
        cross_flow.title = "Verified product".into();
        assert!(product_matches_query(&cross_flow, "crossflow"));
        assert!(product_matches_query(&cross_flow, "cross-flow fans"));
    }

    #[test]
    fn product_query_reuses_the_bounded_admin_query_contract() {
        assert_eq!(
            parse_query_text(Some("  model  ".into())).unwrap(),
            Some("model".into())
        );
        assert!(parse_query_text(Some("x".repeat(201))).is_err());
    }
}

async fn publish_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.product.publish",
        &headers,
        &json!({"actor": &actor, "id": id, "ifMatch": expected_revision}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let product: Product = replay.decode()?;
            return Ok(entity_response(status, &product, product.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let product = state
        .load_working_product(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Product was not found in validated staging."))?;
    if product.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The product changed; reload before publishing.",
        ));
    }
    state.assert_product_publishable(&product).await?;
    if product.status == PublicationStatus::Published
        && product.published_revision == Some(product.current_revision)
        && state
            .product_presentation_is_published(product.id, &product.locale)
            .await?
    {
        let response_product = state.present_product(product).await?;
        idempotency
            .complete(&state, &response_product, StatusCode::OK)
            .await?;
        return Ok(entity_response(
            StatusCode::OK,
            &response_product,
            response_product.current_revision,
        ));
    }
    let now = Utc::now();
    let mut published = product.clone();
    published.status = PublicationStatus::Published;
    published.published_revision = Some(published.current_revision);
    published.updated_at = now;
    state
        .publish_product_projection(&product, &published)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .products
            .insert(id, published.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "product.publish",
        "product",
        Some(id),
        Some(json!(product)),
        Some(json!(published)),
        Some("Publish validated Product Master revision".into()),
    )
    .await?;
    let response_product = state.present_product(published).await?;
    idempotency
        .complete(&state, &response_product, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &response_product,
        response_product.current_revision,
    ))
}

async fn get_product_publication_report(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
) -> Result<Json<crate::models::ProductPublicationReport>, ApiError> {
    let product = state
        .load_working_product(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Product was not found in validated staging."))?;
    let issues = state.product_publication_issues(&product).await?;
    let allowed_actions = if issues.is_empty() && principal.has_permission("product.publish") {
        vec![crate::models::ProductPublicationAction::Publish]
    } else {
        Vec::new()
    };
    Ok(Json(crate::models::ProductPublicationReport {
        product_id: product.id,
        current_revision: product.current_revision,
        ready: issues.is_empty(),
        issues,
        allowed_actions,
    }))
}
