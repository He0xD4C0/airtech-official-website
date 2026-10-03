use super::*;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct IdentityResetRequest {
    reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TemporaryPasswordResult {
    temporary_password: String,
    must_change_password: bool,
    sessions_revoked: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecoveryKeyResetResult {
    recovery_key: String,
    origin: &'static str,
    confirmed: bool,
}

fn secret_json(status: StatusCode, value: &impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Readable one-time password: unambiguous alphabet so it can be dictated, and
/// at least one letter and one digit to satisfy the password policy.
fn temporary_password() -> Result<String, ApiError> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut bytes = [0_u8; 20];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ApiError::internal("Temporary password generation failed."))?;
    let mut password: String = bytes
        .iter()
        .map(|byte| char::from(ALPHABET[*byte as usize % ALPHABET.len()]))
        .collect();
    password.push('7');
    password.push('k');
    Ok(password)
}

fn reset_scope(principal: &AdminPrincipal, id: Uuid) -> Result<(), ApiError> {
    if principal.user_id == id {
        // A reset is an emergency action for another account; this session can
        // already rotate its own password and TOTP from the security page.
        return Err(ApiError::conflict(
            "Use the account security page to reset your own credentials.",
        ));
    }
    Ok(())
}

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
    Json(mut update): Json<UpdateAdminUser>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    if let Some(phone) = update.phone_e164.as_deref() {
        if !principal.is_super_admin() {
            return Err(ApiError::forbidden(
                "Only a Super Admin can bind a phone number for another administrator.",
            ));
        }
        update.phone_e164 = Some(airtek_runtime::auth::normalize_e164(phone)?);
    }
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
        airtek_runtime::services::identity::validate_user_manage_scope(
            &mut transaction,
            principal.is_super_admin(),
            &principal.permissions,
            id,
        )
        .await?;
        if update.role_keys.is_some() && id == principal.user_id {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The current administrator cannot change their own role assignment.",
            ));
        }
        if let Some(role_keys) = &update.role_keys {
            airtek_runtime::services::identity::validate_role_grant(
                &mut transaction,
                principal.is_super_admin(),
                &principal.permissions,
                role_keys,
            )
            .await?;
        }
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

pub(super) async fn reset_user_password(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<IdentityResetRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.user.password_reset",
        &headers,
        &json!({"userId": id}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let result: TemporaryPasswordResult = replay.decode()?;
            return Ok(secret_json(replay.status()?, &result));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let temporary = temporary_password()?;
    let hash = airtek_runtime::auth::hash_secret(&temporary)?;
    let mut transaction = state.pool.begin().await?;
    reset_scope(&principal, id)?;
    airtek_runtime::services::identity::prepare_user_update(&mut transaction, id).await?;
    airtek_runtime::services::identity::validate_user_manage_scope(
        &mut transaction,
        principal.is_super_admin(),
        &principal.permissions,
        id,
    )
    .await?;
    let revoked =
        airtek_runtime::services::identity::reset_user_password(&mut transaction, id, &hash)
            .await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.user.password_reset",
        "user",
        Some(id),
        None,
        Some(json!({"mustChangePassword": true, "sessionsRevoked": revoked})),
        Some(request.reason),
    );
    airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    let result = TemporaryPasswordResult {
        temporary_password: temporary,
        must_change_password: true,
        sessions_revoked: revoked > 0,
    };
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &result, StatusCode::OK)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(secret_json(StatusCode::OK, &result))
}

pub(super) async fn reset_user_totp(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<IdentityResetRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.user.totp_reset",
        &headers,
        &json!({"userId": id}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let user: AdminUserRecord = replay.decode()?;
            return Ok(entity_response(replay.status()?, &user, user.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let mut transaction = state.pool.begin().await?;
    reset_scope(&principal, id)?;
    airtek_runtime::services::identity::prepare_user_update(&mut transaction, id).await?;
    airtek_runtime::services::identity::validate_user_manage_scope(
        &mut transaction,
        principal.is_super_admin(),
        &principal.permissions,
        id,
    )
    .await?;
    airtek_runtime::services::identity::clear_user_totp(&mut transaction, id).await?;
    let revoked =
        airtek_runtime::services::identity::revoke_user_sessions(&mut transaction, id).await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.user.totp_reset",
        "user",
        Some(id),
        None,
        Some(json!({"totpCleared": true, "sessionsRevoked": revoked})),
        Some(request.reason),
    );
    airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    let after = load_admin_user_in_transaction(&mut transaction, id).await?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(entity_response(StatusCode::OK, &after, after.revision))
}

/// Rotates the offline recovery key of the root administrator. Only the root
/// account itself may call this, the key file is the single plaintext source of
/// truth, and the new plaintext is returned exactly once: the idempotency store
/// keeps a marker instead of the key, so a replay answers with a conflict.
pub(super) async fn reset_user_recovery_key(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<IdentityResetRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    if id != principal.user_id {
        return Err(ApiError::forbidden(
            "The administrator recovery key can only be rotated by its owner.",
        ));
    }
    let root_email = state
        .config
        .admin_email
        .as_deref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "AIRTEK_ADMIN_EMAIL is required to rotate the recovery key.",
            )
        })?
        .trim()
        .to_ascii_lowercase();
    if principal.email.trim().to_ascii_lowercase() != root_email {
        return Err(ApiError::forbidden(
            "Only the root administrator holds an administrator recovery key.",
        ));
    }
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.user.recovery_reset",
        &headers,
        &json!({"userId": id}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(_) => {
            // The plaintext key is never persisted in the replay record.
            return Err(ApiError::conflict(
                "This recovery-key rotation was already processed; the new key is displayed only once.",
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let rotated = airtek_runtime::services::admin_provisioning::rotate_recovery_key(
        &state.pool,
        &state.config,
    )
    .await?;
    let mut transaction = state.pool.begin().await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.user.recovery_reset",
        "user",
        Some(id),
        None,
        Some(json!({"origin": "generated", "confirmed": false, "keyFile": rotated.path})),
        Some(request.reason),
    );
    airtek_runtime::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    transaction.commit().await?;
    idempotency
        .complete(
            &state,
            &json!({"rotated": true, "userId": id}),
            StatusCode::OK,
        )
        .await?;
    Ok(secret_json(
        StatusCode::OK,
        &RecoveryKeyResetResult {
            recovery_key: rotated.recovery_key,
            origin: "generated",
            confirmed: false,
        },
    ))
}
