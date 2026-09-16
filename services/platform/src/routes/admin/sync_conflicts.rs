use super::*;

pub(super) async fn resolve_conflict(
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
    let mut transaction = state.pool.begin().await?;
    let conflict = crate::services::admin_sync::resolve_conflict_in_transaction(
        &mut transaction,
        id,
        &request,
        &principal.email,
    )
    .await?;
    let audit = AuditEvent {
        id: Uuid::new_v4(),
        actor: principal.email.clone(),
        action: "feishu.conflict.resolve".into(),
        entity_type: "syncConflict".into(),
        entity_id: Some(id),
        before: None,
        after: Some(json!({"decision": request.decision, "revision": 2})),
        reason: Some(request.reason),
        current_version: Some(2),
        request_id: headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(Uuid::new_v4),
        occurred_at: Utc::now(),
    };
    crate::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &conflict, StatusCode::OK)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(entity_response(StatusCode::OK, &conflict, 2))
}

pub(super) fn validate_conflict_resolution(
    request: &crate::models::ResolveSyncConflictRequest,
) -> Result<(), ApiError> {
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    if !(10..=2000).contains(&request.reason.trim().chars().count()) {
        errors.insert(
            "reason".into(),
            vec!["Reason must contain 10 to 2000 characters.".into()],
        );
    }
    if request.decision == crate::models::SyncConflictDecision::KeepVerifiedLocal {
        if request
            .evidence_reference
            .as_ref()
            .is_none_or(|value| value.trim().is_empty())
        {
            errors.insert(
                "evidenceReference".into(),
                vec![
                    "Evidence reference is required when keeping the verified local value.".into(),
                ],
            );
        }
        if request.expires_at.is_none_or(|value| value <= Utc::now()) {
            errors.insert(
                "expiresAt".into(),
                vec!["A future expiry is required when keeping the verified local value.".into()],
            );
        }
    } else if request.evidence_reference.is_some() || request.expires_at.is_some() {
        errors.insert(
            "decision".into(),
            vec!["Evidence and expiry are only accepted for keepVerifiedLocal.".into()],
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}
