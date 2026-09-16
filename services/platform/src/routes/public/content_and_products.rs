use super::*;

pub(super) async fn get_content(
    State(state): State<AppState>,
    Path((kind, slug)): Path<(String, String)>,
    Query(query): Query<ContentQuery>,
) -> Result<Response, ApiError> {
    let kind = parse_content_kind(&kind)?;
    let entry = crate::routes::public_data::load_v2_content_by_kind_slug(
        &state,
        kind,
        &slug,
        &query.locale,
    )
    .await?
    .ok_or_else(|| ApiError::not_found("Published content was not found."))?;
    let revision = entry.published_revision;
    Ok(with_etag(entry, revision))
}

pub(super) async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductQuery>,
) -> Result<Json<CursorPage<Product>>, ApiError> {
    let limit = query.limit.unwrap_or(24);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let after = query
        .cursor
        .as_deref()
        .map(|value| decode_product_cursor(&state, value, &query))
        .transpose()?;
    let mut products = load_published_product_rows(
        &state.pool,
        query.family,
        query.motor_technology.as_deref(),
        None,
        None,
        after
            .as_ref()
            .map(|value| (value.stable_id.as_str(), value.id.unwrap_or(Uuid::nil()))),
        Some(limit + 1),
    )
    .await?;
    let has_more = products.len() > limit;
    products.truncate(limit);
    let next_cursor = has_more
        .then(|| products.last())
        .flatten()
        .map(|product| encode_product_cursor(product, &query))
        .transpose()?;
    Ok(Json(CursorPage {
        items: products,
        next_cursor,
    }))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProductCursor {
    pub(super) version: u8,
    pub(super) stable_id: String,
    pub(super) id: Option<Uuid>,
    pub(super) family: Option<ProductFamily>,
    pub(super) motor_technology: Option<String>,
}

pub(super) fn encode_product_cursor(
    product: &Product,
    query: &ProductQuery,
) -> Result<String, ApiError> {
    let cursor = ProductCursor {
        version: 2,
        stable_id: product.stable_id.clone(),
        id: Some(product.id),
        family: query.family,
        motor_technology: query.motor_technology.clone(),
    };
    serde_json::to_vec(&cursor)
        .map(|value| URL_SAFE_NO_PAD.encode(value))
        .map_err(|_| ApiError::internal("Product cursor serialization failed."))
}

pub(super) fn decode_product_cursor(
    state: &AppState,
    value: &str,
    query: &ProductQuery,
) -> Result<ProductCursor, ApiError> {
    if value.is_empty() || value.len() > 2_048 {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<ProductCursor>(&value).ok())
        .filter(|cursor| {
            matches!(cursor.version, 1 | 2)
                && !cursor.stable_id.is_empty()
                && cursor.stable_id.len() <= 500
                && cursor.family == query.family
                && cursor.motor_technology == query.motor_technology
        })
        .ok_or_else(|| {
            ApiError::bad_request("cursor is invalid or belongs to different product filters.")
        })?;
    if decoded.version == 1 {
        state.request_metrics.record_legacy_cursor(
            crate::services::request_metrics::LegacyCursorEndpoint::PublicProducts,
        );
    }
    Ok(decoded)
}

pub(super) async fn get_product(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ProductDetailQuery>,
) -> Result<Response, ApiError> {
    let products = load_published_product_rows(
        &state.pool,
        query.family,
        None,
        Some(&slug),
        None,
        None,
        Some(2),
    )
    .await?;
    let product = require_unique_published_product(products)?;
    let revision = product
        .published_revision
        .unwrap_or(product.current_revision);
    Ok(with_etag(product, revision))
}

pub(super) fn require_unique_published_product<T>(mut products: Vec<T>) -> Result<T, ApiError> {
    match products.len() {
        0 => Err(ApiError::not_found("Published product was not found.")),
        1 => Ok(products.pop().expect("one product remains")),
        _ => Err(ApiError::conflict(
            "The product slug is shared by more than one family; include the family query parameter.",
        )),
    }
}

pub(super) async fn select_products(
    State(state): State<AppState>,
    Json(request): Json<SelectorRequest>,
) -> Result<Json<SelectorResponse>, ApiError> {
    validate_selector(&request)?;
    let products: Vec<Product> = load_published_product_rows(
        &state.pool,
        request.preferred_family,
        request.motor_technology.as_deref(),
        None,
        None,
        None,
        None,
    )
    .await?;

    Ok(Json(crate::services::selector::evaluate(
        &request, &products,
    )))
}
