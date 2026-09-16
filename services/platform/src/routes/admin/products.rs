use super::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProductListQuery {
    pub(super) cursor: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) q: Option<String>,
    pub(super) family: Option<String>,
    pub(super) status: Option<String>,
    pub(super) data_state: Option<String>,
}

pub(super) async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductListQuery>,
) -> Result<Json<crate::models::AdminProductPage>, ApiError> {
    let needle = parse_query_text(query.q)?.map(|value| value.to_lowercase());
    let family = query.family.map(parse_product_family).transpose()?;
    let status = query.status.map(parse_publication_status).transpose()?;
    let data_state = query.data_state.map(parse_product_data_state).transpose()?;
    Ok(Json(
        crate::services::admin_product_query::list_admin_products(
            &state,
            crate::services::admin_product_query::ProductListFilter {
                cursor: query.cursor,
                limit: query.limit,
                search: needle,
                family: family.map(enum_label),
                status: status.map(enum_label),
                data_state: data_state.map(str::to_owned),
            },
        )
        .await?,
    ))
}

pub(super) fn parse_product_family(
    value: String,
) -> Result<crate::models::ProductFamily, ApiError> {
    serde_json::from_value(serde_json::Value::String(value))
        .map_err(|_| ApiError::bad_request("family is not a controlled Product family."))
}

pub(super) fn parse_publication_status(value: String) -> Result<PublicationStatus, ApiError> {
    serde_json::from_value(serde_json::Value::String(value))
        .map_err(|_| ApiError::bad_request("status is not a controlled Product status."))
}

pub(super) fn parse_product_data_state(value: String) -> Result<&'static str, ApiError> {
    match value.as_str() {
        "verified" => Ok("verified"),
        "pending" => Ok("pending"),
        _ => Err(ApiError::bad_request(
            "dataState must be verified or pending.",
        )),
    }
}

pub(super) fn enum_label(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(super) async fn publish_product(
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

pub(super) async fn get_product_publication_report(
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

#[cfg(test)]
mod product_query_tests {
    use super::*;

    #[test]
    fn product_query_reuses_the_bounded_admin_query_contract() {
        assert_eq!(
            parse_query_text(Some("  model  ".into())).unwrap(),
            Some("model".into())
        );
        assert!(parse_query_text(Some("x".repeat(201))).is_err());
    }
}
