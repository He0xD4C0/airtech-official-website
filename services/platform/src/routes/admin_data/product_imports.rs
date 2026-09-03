#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductImportAccepted {
    operation_id: Uuid,
    status: String,
    operation_url: String,
    events_url: String,
}

async fn import_products(
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
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Product Master import requires PostgreSQL persistence.")
    })?;
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

async fn list_product_imports(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<ProductImportResult>>, ApiError> {
    let mut values = if let Some(pool) = &state.pool {
        load_product_imports(pool).await?
    } else {
        state
            .data
            .read()
            .await
            .product_imports
            .values()
            .cloned()
            .collect()
    };
    values.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    Ok(Json(paginate_by_id(
        "admin.productImports",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn load_product_imports(pool: &sqlx::PgPool) -> Result<Vec<ProductImportResult>, ApiError> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM product_import_runs ORDER BY created_at DESC,id",
    )
    .fetch_all(pool)
    .await?;
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(result) = load_stored_product_import(pool, id).await? {
            results.push(result);
        }
    }
    Ok(results)
}

async fn get_product_import(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProductImportResult>, ApiError> {
    let entry = if let Some(pool) = &state.pool {
        load_stored_product_import(pool, id).await?
    } else {
        state.data.read().await.product_imports.get(&id).cloned()
    }
    .ok_or_else(|| ApiError::not_found("Product import was not found."))?;
    Ok(Json(entry))
}

async fn get_admin_product(
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

async fn get_private_pricing(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Private Product Master pricing requires PostgreSQL.")
    })?;
    let key = state
        .config
        .product_staging_encryption_key
        .as_ref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Product staging encryption is not configured; pricing cannot be decrypted.",
            )
        })?;
    let row = sqlx::query(
        r#"SELECT product.stable_id,staging.source_row_number,staging.nonce,
                  staging.ciphertext,staging.authentication_tag,
                  import_run.mapping_version,import_run.source_checksum
           FROM products product
           JOIN product_import_private_staging staging
             ON staging.import_run_id=product.product_import_run_id
            AND staging.source_record_id=product.stable_id
           JOIN product_import_runs import_run ON import_run.id=staging.import_run_id
           WHERE product.id=$1 AND product.data_origin='verifiedCsv'
             AND staging.status IN ('validated','promoted')
             AND staging.expires_at > now()
           ORDER BY staging.created_at DESC LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Private pricing was not found for this product."))?;
    let stable_id: String = row.try_get("stable_id")?;
    let source_row_number: i32 = row.try_get("source_row_number")?;
    let nonce: Vec<u8> = row.try_get("nonce")?;
    let ciphertext: Vec<u8> = row.try_get("ciphertext")?;
    let authentication_tag: Vec<u8> = row.try_get("authentication_tag")?;
    let mapping_version: String = row.try_get("mapping_version")?;
    let checksum: String = row.try_get("source_checksum")?;
    let pricing_fields = crate::services::product_import::decrypt_private_pricing(
        key,
        crate::services::product_import::PrivatePricingEnvelope {
            mapping_version: &mapping_version,
            checksum: &checksum,
            source_row_number,
            stable_id: &stable_id,
            nonce: &nonce,
            ciphertext: &ciphertext,
            authentication_tag: &authentication_tag,
        },
    )?;
    let result = ProductPrivatePricing {
        product_id: id,
        stable_id,
        source_row_number,
        pricing_fields,
    };
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
