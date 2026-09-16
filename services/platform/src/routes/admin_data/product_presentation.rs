use super::*;

pub(super) async fn update_product_presentation(
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
    let after = {
        let mut transaction = state.pool.begin().await?;
        let write_context = crate::services::admin_products::lock_product_presentation(
            &mut transaction,
            id,
            before.product.current_revision,
            expected,
            &input,
        )
        .await?;
        let presentation = crate::services::admin_products::write_product_presentation(
            &mut transaction,
            id,
            before.product.current_revision,
            expected,
            &input,
            &actor_name,
            write_context,
            now,
        )
        .await?;
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
        crate::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
        let staged = idempotency
            .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
            .await?;
        transaction.commit().await?;
        staged.finish().await?;
        after
    };
    let revision = after
        .presentation
        .as_ref()
        .map(|presentation| presentation.revision)
        .unwrap_or_default();
    Ok(entity_response(StatusCode::OK, &after, revision))
}
