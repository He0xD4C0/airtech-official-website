#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
    family: Option<String>,
    status: Option<String>,
    data_state: Option<String>,
}

async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductListQuery>,
) -> Result<Json<crate::models::AdminProductPage>, ApiError> {
    let needle = parse_query_text(query.q)?.map(|value| value.to_lowercase());
    let family = query.family.map(parse_product_family).transpose()?;
    let status = query.status.map(parse_publication_status).transpose()?;
    let data_state = query.data_state.map(parse_product_data_state).transpose()?;
    let mut values = state.list_working_products().await?;
    let input_records = values.len();
    let started = std::time::Instant::now();
    if let Some(needle) = &needle {
        values.retain(|product| product_matches_query(product, needle));
    }
    if let Some(status) = status {
        values.retain(|product| product.status == status);
    }
    if let Some(data_state) = data_state {
        values.retain(|product| product_data_state(product) == data_state);
    }
    let family_counts = product_facet_counts(&values, |product| enum_label(product.family));
    let status_counts = product_facet_counts(&values, |product| enum_label(product.status));
    let data_state_counts =
        product_facet_counts(&values, |product| product_data_state(product).into());
    if let Some(family) = family {
        values.retain(|product| product.family == family);
    }
    values.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    let total = values.len();
    let scope = format!("admin.products|{needle:?}|{family:?}|{status:?}|{data_state:?}");
    let page = crate::pagination::paginate_by_id_scoped(
        &scope,
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
    Ok(Json(crate::models::AdminProductPage {
        items: page.items,
        next_cursor: page.next_cursor,
        total,
        family_counts,
        status_counts,
        data_state_counts,
    }))
}

fn parse_product_family(value: String) -> Result<crate::models::ProductFamily, ApiError> {
    serde_json::from_value(serde_json::Value::String(value))
        .map_err(|_| ApiError::bad_request("family is not a controlled Product family."))
}

fn parse_publication_status(value: String) -> Result<PublicationStatus, ApiError> {
    serde_json::from_value(serde_json::Value::String(value))
        .map_err(|_| ApiError::bad_request("status is not a controlled Product status."))
}

fn parse_product_data_state(value: String) -> Result<&'static str, ApiError> {
    match value.as_str() {
        "verified" => Ok("verified"),
        "pending" => Ok("pending"),
        _ => Err(ApiError::bad_request(
            "dataState must be verified or pending.",
        )),
    }
}

fn product_data_state(product: &Product) -> &'static str {
    if product
        .specifications
        .iter()
        .any(|specification| specification.state == crate::models::FactState::PendingVerification)
    {
        "pending"
    } else {
        "verified"
    }
}

fn product_facet_counts(
    products: &[Product],
    value: impl Fn(&Product) -> String,
) -> Vec<crate::models::ProductFacetCount> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for product in products {
        *counts.entry(value(product)).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(value, count)| crate::models::ProductFacetCount { value, count })
        .collect()
}

fn enum_label(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
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

    #[tokio::test]
    async fn product_list_filters_and_paginates_more_than_one_hundred_records() {
        let state = AppState::for_test();
        {
            let mut data = state.data.write().await;
            for index in 0_u128..125 {
                let mut value = product();
                value.id = Uuid::from_u128(index + 1);
                value.stable_id = format!("BULK-{index:03}");
                value.model = Some(format!("Bulk model {index:03}"));
                value.family = if index % 2 == 0 {
                    crate::models::ProductFamily::Axial
                } else {
                    crate::models::ProductFamily::Centrifugal
                };
                data.products.insert(value.id, value);
            }
        }

        let Json(first) = list_products(
            State(state.clone()),
            Query(ProductListQuery {
                cursor: None,
                limit: Some(20),
                q: Some("bulk".into()),
                family: Some("axial".into()),
                status: Some("draft".into()),
                data_state: Some("verified".into()),
            }),
        )
        .await
        .expect("first filtered page");

        assert_eq!(first.total, 63);
        assert_eq!(first.items.len(), 20);
        assert!(first.next_cursor.is_some());
        assert_eq!(
            first
                .family_counts
                .iter()
                .find(|facet| facet.value == "axial")
                .map(|facet| facet.count),
            Some(63)
        );

        let Json(second) = list_products(
            State(state),
            Query(ProductListQuery {
                cursor: first.next_cursor,
                limit: Some(20),
                q: Some("bulk".into()),
                family: Some("axial".into()),
                status: Some("draft".into()),
                data_state: Some("verified".into()),
            }),
        )
        .await
        .expect("second filtered page");
        assert_eq!(second.total, 63);
        assert_eq!(second.items.len(), 20);
        assert!(second
            .items
            .iter()
            .all(|value| value.family == crate::models::ProductFamily::Axial));
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
