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
) -> Result<Json<PublishedProductPage>, ApiError> {
    let limit = query.limit.unwrap_or(24);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let scope = ProductCursorScope {
        family: query.family,
        motor_technology: normalized_filter(query.motor_technology.as_deref(), 120)?,
        q: normalized_search(query.q.as_deref())?,
    };
    let after = query
        .cursor
        .as_deref()
        .map(|value| decode_product_cursor(&state, value, &scope))
        .transpose()?;
    let (mut products, facets) = airtek_runtime::services::public_content::load_catalog_page(
        &state.pool,
        PublishedProductFilter {
            family: scope.family,
            motor_technology: scope.motor_technology.as_deref(),
            search: scope.q.as_deref(),
            after: after
                .as_ref()
                .map(|value| (value.stable_id.as_str(), value.id.unwrap_or(Uuid::nil()))),
            limit: Some(limit + 1),
            ..Default::default()
        },
    )
    .await?;
    let has_more = products.len() > limit;
    products.truncate(limit);
    let next_cursor = has_more
        .then(|| products.last())
        .flatten()
        .map(|product| encode_product_cursor(product, &scope))
        .transpose()?;
    Ok(Json(PublishedProductPage {
        items: products,
        next_cursor,
        total: facets.0,
        family_counts: facets.1,
        motor_technology_counts: facets.2,
    }))
}

#[derive(Debug)]
pub(super) struct ProductCursorScope {
    pub(super) family: Option<ProductFamily>,
    pub(super) motor_technology: Option<String>,
    pub(super) q: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProductCursor {
    pub(super) version: u8,
    pub(super) stable_id: String,
    pub(super) id: Option<Uuid>,
    pub(super) family: Option<ProductFamily>,
    pub(super) motor_technology: Option<String>,
    #[serde(default)]
    pub(super) q: Option<String>,
}

pub(super) fn encode_product_cursor(
    product: &Product,
    scope: &ProductCursorScope,
) -> Result<String, ApiError> {
    let cursor = ProductCursor {
        version: 3,
        stable_id: product.stable_id.clone(),
        id: Some(product.id),
        family: scope.family,
        motor_technology: scope.motor_technology.clone(),
        q: scope.q.clone(),
    };
    serde_json::to_vec(&cursor)
        .map(|value| URL_SAFE_NO_PAD.encode(value))
        .map_err(|_| ApiError::internal("Product cursor serialization failed."))
}

pub(super) fn decode_product_cursor(
    state: &AppState,
    value: &str,
    scope: &ProductCursorScope,
) -> Result<ProductCursor, ApiError> {
    if value.is_empty() || value.len() > 2_048 {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<ProductCursor>(&value).ok())
        .filter(|cursor| {
            matches!(cursor.version, 2 | 3)
                && !cursor.stable_id.is_empty()
                && cursor.stable_id.len() <= 500
                && cursor.family == scope.family
                && cursor.motor_technology == scope.motor_technology
                && ((cursor.version == 2 && scope.q.is_none() && cursor.q.is_none())
                    || (cursor.version == 3 && cursor.q == scope.q))
        })
        .ok_or_else(|| {
            ApiError::bad_request("cursor is invalid or belongs to different product filters.")
        })?;
    if decoded.version == 2 {
        state.request_metrics.record_legacy_cursor(
            airtek_runtime::services::request_metrics::LegacyCursorEndpoint::PublicProducts,
        );
    }
    Ok(decoded)
}

fn normalized_filter(
    value: Option<&str>,
    maximum_length: usize,
) -> Result<Option<String>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > maximum_length || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request("product filter is invalid."));
    }
    Ok(Some(value.to_owned()))
}

fn normalized_search(value: Option<&str>) -> Result<Option<String>, ApiError> {
    Ok(normalized_filter(value, 200)?.map(|value| {
        value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }))
}

pub(super) async fn get_product(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ProductDetailQuery>,
) -> Result<Response, ApiError> {
    let products = load_published_product_rows(
        &state.pool,
        PublishedProductFilter {
            family: query.family,
            slug: Some(&slug),
            limit: Some(2),
            ..Default::default()
        },
    )
    .await?;
    let product = require_unique_published_product(products)?;
    let revision = product
        .published_revision
        .unwrap_or(product.current_revision);
    Ok(with_etag(product, revision))
}

pub(super) async fn get_product_assets(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ProductDetailQuery>,
) -> Result<Response, ApiError> {
    let document = airtek_runtime::services::public_content::load_published_product_assets(
        &state.pool,
        &slug,
        query.family,
    )
    .await?;
    let revision = document.product_revision;
    Ok(with_etag(document, revision))
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
        PublishedProductFilter {
            family: request.preferred_family,
            motor_technology: request.motor_technology.as_deref(),
            ..Default::default()
        },
    )
    .await?;

    Ok(Json(airtek_runtime::services::selector::evaluate(
        &request, &products,
    )))
}

#[cfg(test)]
mod cursor_tests {
    use serde_json::json;

    use super::*;
    use airtek_runtime::config::Config;

    fn state() -> AppState {
        let mut config = Config::for_test();
        config.database_url = Some("postgres://airtek:airtek@127.0.0.1/airtek".into());
        AppState::new(config).expect("lazy PostgreSQL test state")
    }

    fn cursor(value: Value) -> String {
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value).unwrap())
    }

    #[tokio::test]
    async fn v3_product_cursor_binds_normalized_search_and_filters() {
        let id = Uuid::new_v4();
        let value = cursor(json!({
            "version": 3,
            "stableId": "AT-TEST-001",
            "id": id,
            "family": "axial",
            "motorTechnology": "EC",
            "q": "verified 230"
        }));
        let scope = ProductCursorScope {
            family: Some(ProductFamily::Axial),
            motor_technology: Some("EC".into()),
            q: Some("verified 230".into()),
        };
        assert_eq!(
            decode_product_cursor(&state(), &value, &scope).unwrap().id,
            Some(id)
        );
        assert!(decode_product_cursor(
            &state(),
            &value,
            &ProductCursorScope {
                q: Some("different".into()),
                ..scope
            }
        )
        .is_err());
    }

    #[tokio::test]
    async fn v2_product_cursor_is_accepted_only_without_search() {
        let value = cursor(json!({
            "version": 2,
            "stableId": "AT-TEST-001",
            "id": Uuid::new_v4(),
            "family": null,
            "motorTechnology": null
        }));
        let scope = ProductCursorScope {
            family: None,
            motor_technology: None,
            q: None,
        };
        assert!(decode_product_cursor(&state(), &value, &scope).is_ok());
        assert!(decode_product_cursor(
            &state(),
            &value,
            &ProductCursorScope {
                q: Some("query".into()),
                ..scope
            }
        )
        .is_err());
    }

    #[test]
    fn product_search_is_bounded_and_normalized() {
        assert_eq!(
            normalized_search(Some("  Verified   MODEL  "))
                .unwrap()
                .as_deref(),
            Some("verified model")
        );
        assert!(normalized_search(Some(&"x".repeat(201))).is_err());
        assert!(normalized_search(Some("model\nsecret")).is_err());
    }
}
