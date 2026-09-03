async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<Product>>, ApiError> {
    let mut values = state.list_working_products().await?;
    values.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    Ok(Json(paginate_by_id(
        "admin.products",
        values,
        query,
        |product| product.id,
    )?))
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
