use super::*;

pub(super) async fn list_users(
    State(state): State<AppState>,
    Query(query): Query<IdentityListQuery>,
) -> Result<Json<airtek_domain::models::AdminUserPage>, ApiError> {
    let search = identity_query_text(query.q)?;
    let status = query
        .status
        .map(|value| match value.as_str() {
            "invited" | "active" | "disabled" => Ok(value),
            _ => Err(ApiError::bad_request(
                "status is not a controlled user state.",
            )),
        })
        .transpose()?;
    Ok(Json(
        airtek_runtime::services::identity::list_users(
            &state,
            search.as_deref(),
            status.as_deref(),
            CursorQuery {
                cursor: query.cursor,
                limit: query.limit,
            },
        )
        .await?,
    ))
}

pub(super) async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let user = load_admin_user(&state, id).await?;
    Ok(entity_response(StatusCode::OK, &user, user.revision))
}

pub(super) async fn update_user(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(update): Json<UpdateAdminUser>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    if id == principal.user_id
        && update
            .status
            .as_deref()
            .is_some_and(|status| status != "active")
    {
        return Err(ApiError::conflict(
            "The current administrator cannot make their own account non-active.",
        ));
    }
    if update
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "invited" | "active" | "disabled"))
    {
        return Err(ApiError::bad_request("status is invalid."));
    }
    let idempotency_request = json!({"userId": id, "update": &update});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.user.update",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let user: AdminUserRecord = replay.decode()?;
            return Ok(entity_response(status, &user, user.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_admin_user(&state, id).await?;
    if before.revision != expected {
        return Err(ApiError::conflict(
            "The administrator changed; reload before saving.",
        ));
    }
    let after = {
        let pool = &state.pool;
        let mut transaction = pool.begin().await?;
        let target =
            airtek_runtime::services::identity::prepare_user_update(&mut transaction, id).await?;
        let current_status = target.status;
        let is_super_admin = target.is_super_admin;
        let target_status_is_active =
            update.status.as_deref().unwrap_or(&current_status) == "active";
        let target_keeps_super_admin = update
            .role_keys
            .as_ref()
            .map(|roles| roles.iter().any(|role| role == "super-admin"))
            .unwrap_or(is_super_admin);
        let removes_super_admin = current_status == "active"
            && is_super_admin
            && (!target_status_is_active || !target_keeps_super_admin);
        if removes_super_admin {
            let active_super_admin_count =
                airtek_runtime::services::identity::active_super_admin_count(&mut transaction)
                    .await?;
            if active_super_admin_count <= 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The last active Super Admin cannot become non-active or lose that role.",
                ));
            }
        }
        if airtek_runtime::services::identity::update_user_record(
            &mut transaction,
            id,
            &update,
            expected,
        )
        .await?
            != 1
        {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The administrator changed; reload before saving.",
            ));
        }
        if let Some(role_keys) = &update.role_keys {
            airtek_runtime::services::identity::replace_user_roles(&mut transaction, id, role_keys)
                .await?;
        }
        if let Some(status) = &update.status {
            if status != &before.status {
                airtek_runtime::services::identity::insert_user_status_history(
                    &mut transaction,
                    airtek_runtime::services::identity::UserStatusHistory {
                        user_id: id,
                        from_status: &before.status,
                        to_status: status,
                        reason: &update.reason,
                        changed_by: principal.user_id,
                        request_id: request_id(&headers),
                    },
                )
                .await?;
            }
        }
        let after = load_admin_user_in_transaction(&mut transaction, id).await?;
        let audit = mutation_audit_event(
            &headers,
            "identity.user.update",
            "user",
            Some(id),
            Some(json!(before)),
            Some(json!(after)),
            Some(update.reason),
        );
        airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit)
            .await?;
        let staged = idempotency
            .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
            .await?;
        transaction.commit().await?;
        staged.finish().await?;
        after
    };
    Ok(entity_response(StatusCode::OK, &after, after.revision))
}

pub(super) async fn revoke_user_sessions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let sessions_revoked =
        airtek_runtime::services::identity::revoke_user_sessions(&mut transaction, id).await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.sessions.revoke",
        "user",
        Some(id),
        None,
        Some(json!({"sessionsRevoked": sessions_revoked})),
        Some("Revoke all sessions for administrator".into()),
    );
    airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    transaction.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
