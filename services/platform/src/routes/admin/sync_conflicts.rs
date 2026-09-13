async fn resolve_conflict(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<crate::models::ResolveSyncConflictRequest>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    if expected != 1 {
        return Err(ApiError::conflict("The sync conflict is no longer open.")
            .with_code("sync_conflict_resolved"));
    }
    validate_conflict_resolution(&request)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.feishu.conflict.resolve",
        &headers,
        &json!({"actor": principal.user_id, "id": id, "ifMatch": expected, "request": request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let conflict: crate::models::SyncConflict = replay.decode()?;
            return Ok(entity_response(status, &conflict, conflict.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("PostgreSQL is required to resolve sync conflicts.")
    })?;
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"SELECT id,sync_run_id,product_id,source_record_id,field_diffs,resolved_at,resolution
           FROM sync_conflicts WHERE id=$1 FOR UPDATE"#,
    )
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Sync conflict was not found."))?;
    if row.try_get::<Option<DateTime<Utc>>, _>("resolved_at")?.is_some() {
        return Err(ApiError::conflict("The sync conflict is already resolved.")
            .with_code("sync_conflict_resolved"));
    }
    let product_id: Option<Uuid> = row.try_get("product_id")?;
    let diffs: Vec<crate::models::FieldDiff> = serde_json::from_value(row.try_get("field_diffs")?)
        .map_err(|_| ApiError::service_unavailable("Stored sync conflict data is invalid."))?;
    if request.decision == crate::models::SyncConflictDecision::KeepVerifiedLocal {
        insert_verified_local_overrides(
            &mut transaction,
            product_id,
            &diffs,
            &request,
            &principal.email,
        )
        .await?;
    }
    let resolved_at = Utc::now();
    sqlx::query(
        r#"UPDATE sync_conflicts SET resolved_at=$2,resolution=$3,resolved_by=$4
           WHERE id=$1 AND resolved_at IS NULL"#,
    )
    .bind(id)
    .bind(resolved_at)
    .bind(request.decision.label())
    .bind(&principal.email)
    .execute(&mut *transaction)
    .await?;
    let sync_run_id: Uuid = row.try_get("sync_run_id")?;
    sqlx::query(
        r#"UPDATE sync_runs SET
             conflict_count=(SELECT count(*) FROM sync_conflicts WHERE sync_run_id=$1 AND resolved_at IS NULL),
             status=CASE WHEN status='awaitingResolution' AND NOT EXISTS(
               SELECT 1 FROM sync_conflicts WHERE sync_run_id=$1 AND resolved_at IS NULL
             ) THEN 'readyToPublish' ELSE status END
           WHERE id=$1"#,
    )
    .bind(sync_run_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    let conflict = crate::models::SyncConflict {
        id,
        sync_run_id,
        product_id,
        source_record_id: row.try_get("source_record_id")?,
        diffs,
        resolved_at: Some(resolved_at),
        resolution: Some(request.decision.label().into()),
        revision: 2,
    };
    audit(
        &state,
        &headers,
        &principal.email,
        "feishu.conflict.resolve",
        "syncConflict",
        Some(id),
        None,
        Some(json!({"decision": request.decision, "revision": 2})),
        Some(request.reason),
    )
    .await?;
    idempotency
        .complete(&state, &conflict, StatusCode::OK)
        .await?;
    Ok(entity_response(StatusCode::OK, &conflict, 2))
}

fn validate_conflict_resolution(
    request: &crate::models::ResolveSyncConflictRequest,
) -> Result<(), ApiError> {
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    if !(10..=2000).contains(&request.reason.trim().chars().count()) {
        errors.insert("reason".into(), vec!["Reason must contain 10 to 2000 characters.".into()]);
    }
    if request.decision == crate::models::SyncConflictDecision::KeepVerifiedLocal {
        if request.evidence_reference.as_ref().is_none_or(|value| value.trim().is_empty()) {
            errors.insert("evidenceReference".into(), vec!["Evidence reference is required when keeping the verified local value.".into()]);
        }
        if request.expires_at.is_none_or(|value| value <= Utc::now()) {
            errors.insert("expiresAt".into(), vec!["A future expiry is required when keeping the verified local value.".into()]);
        }
    } else if request.evidence_reference.is_some() || request.expires_at.is_some() {
        errors.insert("decision".into(), vec!["Evidence and expiry are only accepted for keepVerifiedLocal.".into()]);
    }
    if errors.is_empty() { Ok(()) } else { Err(ApiError::validation(errors)) }
}

async fn insert_verified_local_overrides(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    product_id: Option<Uuid>,
    diffs: &[crate::models::FieldDiff],
    request: &crate::models::ResolveSyncConflictRequest,
    actor: &str,
) -> Result<(), ApiError> {
    let product_id = product_id.ok_or_else(|| ApiError::validation(BTreeMap::from([(
        "productId".into(), vec!["A bound Product is required to keep verified local values.".into()],
    )])))?;
    let evidence = request.evidence_reference.as_deref().unwrap_or_default().trim();
    let expiry = request.expires_at.expect("validated expiry");
    let source_owned = diffs.iter().filter(|diff| diff.source_owned).collect::<Vec<_>>();
    if source_owned.is_empty() {
        return Err(ApiError::validation(BTreeMap::from([(
            "diffs".into(), vec!["No source-owned field can receive a temporary override.".into()],
        )])));
    }
    for diff in source_owned {
        let value = diff.local_value.clone().ok_or_else(|| ApiError::validation(BTreeMap::from([(
            diff.field_path.clone(), vec!["The verified local value is missing; no value will be inferred.".into()],
        )])))?;
        sqlx::query(
            r#"INSERT INTO product_temporary_overrides
               (id,product_id,field_path,value,reason,created_at,expires_at,resolved_by)
               VALUES ($1,$2,$3,$4,$5,now(),$6,$7)"#,
        )
        .bind(Uuid::new_v4()).bind(product_id).bind(&diff.field_path).bind(value)
        .bind(format!("{} Evidence: {}", request.reason.trim(), evidence)).bind(expiry).bind(actor)
        .execute(&mut **transaction).await?;
    }
    Ok(())
}
