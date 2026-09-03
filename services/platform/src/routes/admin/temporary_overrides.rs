async fn list_temporary_overrides(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<TemporaryOverride>>, ApiError> {
    let mut values: Vec<_> = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT id,product_id,field_path,value,reason,created_at,expires_at
               FROM product_temporary_overrides WHERE product_id=$1
               ORDER BY created_at DESC,id"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let expires_at = row.try_get("expires_at")?;
                Ok(TemporaryOverride {
                    id: row.try_get("id")?,
                    product_id: row.try_get("product_id")?,
                    field_path: row.try_get("field_path")?,
                    value: row.try_get("value")?,
                    reason: row.try_get("reason")?,
                    created_at: row.try_get("created_at")?,
                    expires_at,
                    expired: expires_at <= Utc::now(),
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        state
            .data
            .read()
            .await
            .temporary_overrides
            .values()
            .filter(|value| value.product_id == id)
            .cloned()
            .collect()
    };
    values.sort_by_key(|entry| Reverse(entry.created_at));
    Ok(Json(paginate_by_id(
        "admin.productOverrides",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn create_temporary_override(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(mut input): Json<CreateTemporaryOverride>,
) -> Result<Response, ApiError> {
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.product.override.create",
        &headers,
        &json!({"actor": &actor, "id": id, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let value: TemporaryOverride = replay.decode()?;
            return Ok((status, Json(value)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if input.product_id != id {
        return Err(ApiError::bad_request(
            "Path product id must match productId.",
        ));
    }
    if state.load_working_product(id).await?.is_none() {
        return Err(ApiError::not_found("Product was not found."));
    }
    if input.reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "Override reason must contain at least 10 characters.",
        ));
    }
    if input.field_path.trim().is_empty() || input.field_path.starts_with("site.") {
        return Err(ApiError::bad_request(
            "fieldPath must identify a Feishu-owned source field; site-owned fields do not require overrides.",
        ));
    }
    if let Some(expires_at) = input.expires_at {
        if expires_at <= Utc::now() {
            return Err(ApiError::bad_request("expiresAt must be in the future."));
        }
    } else {
        let default_days = state
            .integer_setting("temporaryOverrideDefaultDays", 30, 1, 365)
            .await?;
        input.expires_at = Some(Utc::now() + ChronoDuration::days(default_days));
    }
    input.reason = input.reason.trim().to_owned();
    let value = TemporaryOverride::from_input(input);
    state.persist_override(&value).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .temporary_overrides
            .insert(value.id, value.clone());
    }
    let override_reason = value.reason.clone();
    audit(
        &state,
        &headers,
        &actor,
        "product.override.create",
        "temporaryOverride",
        Some(value.id),
        None,
        Some(json!(value)),
        Some(override_reason),
    )
    .await?;
    idempotency
        .complete(&state, &value, StatusCode::CREATED)
        .await?;
    Ok((StatusCode::CREATED, Json(value)).into_response())
}
