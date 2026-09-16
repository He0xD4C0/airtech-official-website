use super::*;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProductImportAccepted {
    pub(super) operation_id: Uuid,
    pub(super) status: String,
    pub(super) operation_url: String,
    pub(super) events_url: String,
}

pub(super) async fn import_products(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<ProductImportRequest>,
) -> Result<Response, ApiError> {
    if request.csv.is_empty() || request.csv.len() > 16 * 1024 * 1024 {
        return Err(ApiError::bad_request(
            "csv must contain between 1 byte and 16 MiB.",
        ));
    }
    let idempotency =
        match begin_idempotency(&state, "admin.productImport", &headers, &request).await? {
            IdempotencyOutcome::Replay(replay) => {
                let status = replay.status()?;
                let accepted: ProductImportAccepted = replay.decode()?;
                return Ok((status, Json(accepted)).into_response());
            }
            IdempotencyOutcome::Fresh(context) => context,
        };
    let mapping_version = request
        .mapping_version
        .unwrap_or_else(|| state.config.product_import_mapping_version.clone());
    let pool = &state.pool;
    let actor_id = principal.user_id;
    let key = state.config.product_staging_encryption_key.clone();
    let parsed = tokio::task::spawn_blocking(move || {
        parse_product_master(&request.csv, &mapping_version, key.as_ref())
    })
    .await
    .map_err(|_| ApiError::internal("Product import validation task failed."))??;
    let staged = stage_and_queue_product_import(
        pool,
        parsed,
        state.environment_label(),
        state.config.approved_product_master.as_ref(),
        Some(actor_id),
        &actor(&headers),
    )
    .await?;
    let accepted = ProductImportAccepted {
        operation_id: staged.operation_id,
        status: if staged.queued {
            "queued".into()
        } else {
            staged.result.status.clone()
        },
        operation_url: format!("/api/admin/v1/operations/{}", staged.operation_id),
        events_url: format!("/api/admin/v1/operations/{}/events", staged.operation_id),
    };
    idempotency
        .complete(&state, &accepted, StatusCode::ACCEPTED)
        .await?;
    let mut response = (StatusCode::ACCEPTED, Json(accepted)).into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&format!("/api/admin/v1/operations/{}", staged.operation_id))
            .expect("operation URL is valid"),
    );
    Ok(response)
}

pub(super) async fn list_product_imports(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<ProductImportResult>>, ApiError> {
    Ok(Json(
        crate::services::admin_products::list_product_imports(&state, query).await?,
    ))
}

pub(super) async fn get_product_import(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProductImportResult>, ApiError> {
    let entry = load_stored_product_import(&state.pool, id)
        .await?
        .ok_or_else(|| ApiError::not_found("Product import was not found."))?;
    Ok(Json(entry))
}

pub(super) async fn get_admin_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let detail = load_admin_product_detail(&state, id).await?;
    let presentation_revision = detail
        .presentation
        .as_ref()
        .map(|presentation| presentation.revision)
        .unwrap_or_default();
    Ok(entity_response(
        StatusCode::OK,
        &detail,
        presentation_revision,
    ))
}

pub(super) async fn get_private_pricing(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let result = crate::services::admin_products::load_private_pricing(&state, id).await?;
    audit_mutation(
        &state,
        &headers,
        "product.privatePricing.read",
        "product",
        Some(id),
        None,
        Some(json!({
            "sourceRowNumber": result.source_row_number,
            "pricingFieldCount": result.pricing_fields.len()
        })),
        Some("Read encrypted Product Master pricing fields".into()),
    )
    .await?;
    let mut response = (StatusCode::OK, Json(result)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}
