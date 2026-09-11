async fn get_content(
    State(state): State<AppState>,
    Path((kind, slug)): Path<(String, String)>,
    Query(query): Query<ContentQuery>,
) -> Result<Response, ApiError> {
    let kind = parse_content_kind(&kind)?;
    let entry = super::public_data::load_v2_content_by_kind_slug(
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

async fn list_products(
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
        .map(|value| decode_product_cursor(value, &query))
        .transpose()?;
    let mut products: Vec<_> = if let Some(pool) = &state.pool {
        load_published_product_rows(pool, query.family, query.motor_technology.as_deref(), None)
            .await?
    } else {
        state
            .data
            .read()
            .await
            .published_products
            .values()
            .filter(|product| {
                query
                    .family
                    .map(|family| family == product.family)
                    .unwrap_or(true)
                    && query
                        .motor_technology
                        .as_ref()
                        .map(|technology| product.motor_technology.as_ref() == Some(technology))
                        .unwrap_or(true)
            })
            .cloned()
            .collect()
    };
    products.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    if let Some(after) = after {
        products.retain(|product| product.stable_id > after);
    }
    let has_more = products.len() > limit;
    products.truncate(limit);
    let next_cursor = has_more
        .then(|| products.last())
        .flatten()
        .map(|product| encode_product_cursor(&product.stable_id, &query))
        .transpose()?;
    Ok(Json(CursorPage {
        items: products,
        next_cursor,
    }))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductCursor {
    version: u8,
    stable_id: String,
    family: Option<ProductFamily>,
    motor_technology: Option<String>,
}

fn encode_product_cursor(stable_id: &str, query: &ProductQuery) -> Result<String, ApiError> {
    let cursor = ProductCursor {
        version: 1,
        stable_id: stable_id.to_owned(),
        family: query.family,
        motor_technology: query.motor_technology.clone(),
    };
    serde_json::to_vec(&cursor)
        .map(|value| URL_SAFE_NO_PAD.encode(value))
        .map_err(|_| ApiError::internal("Product cursor serialization failed."))
}

fn decode_product_cursor(value: &str, query: &ProductQuery) -> Result<String, ApiError> {
    if value.is_empty() || value.len() > 2_048 {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<ProductCursor>(&value).ok())
        .filter(|cursor| {
            cursor.version == 1
                && !cursor.stable_id.is_empty()
                && cursor.stable_id.len() <= 500
                && cursor.family == query.family
                && cursor.motor_technology == query.motor_technology
        })
        .ok_or_else(|| {
            ApiError::bad_request("cursor is invalid or belongs to different product filters.")
        })?;
    Ok(decoded.stable_id)
}

async fn get_product(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ProductDetailQuery>,
) -> Result<Response, ApiError> {
    let products = if let Some(pool) = &state.pool {
        load_published_product_rows(pool, query.family, None, Some(&slug)).await?
    } else {
        state
            .data
            .read()
            .await
            .published_products
            .values()
            .filter(|product| {
                product.slug == slug
                    && query
                        .family
                        .map(|family| product.family == family)
                        .unwrap_or(true)
            })
            .cloned()
            .collect()
    };
    let product = require_unique_published_product(products)?;
    let revision = product
        .published_revision
        .unwrap_or(product.current_revision);
    Ok(with_etag(product, revision))
}

fn require_unique_published_product<T>(mut products: Vec<T>) -> Result<T, ApiError> {
    match products.len() {
        0 => Err(ApiError::not_found("Published product was not found.")),
        1 => Ok(products.pop().expect("one product remains")),
        _ => Err(ApiError::conflict(
            "The product slug is shared by more than one family; include the family query parameter.",
        )),
    }
}

async fn select_products(
    State(state): State<AppState>,
    Json(request): Json<SelectorRequest>,
) -> Result<Json<SelectorResponse>, ApiError> {
    validate_selector(&request)?;
    let products: Vec<Product> = if let Some(pool) = &state.pool {
        load_published_product_rows(
            pool,
            request.preferred_family,
            request.motor_technology.as_deref(),
            None,
        )
        .await?
    } else {
        state
            .data
            .read()
            .await
            .published_products
            .values()
            .filter(|product| {
                request
                    .preferred_family
                    .map(|family| family == product.family)
                    .unwrap_or(true)
                    && request
                        .motor_technology
                        .as_ref()
                        .map(|technology| product.motor_technology.as_ref() == Some(technology))
                        .unwrap_or(true)
            })
            .cloned()
            .collect()
    };

    Ok(Json(crate::services::selector::evaluate(
        &request, &products,
    )))
}
