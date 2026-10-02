use super::*;

pub async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChangePasswordRequest>,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    validate_strong_password(&request.new_password)?;
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let current_hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(principal.user_id)
        .fetch_one(&state.pool)
        .await?;
    if !verify_password(&current_hash, &request.current_password) {
        return Err(ApiError::unauthorized("The current password is incorrect."));
    }
    let new_hash = hash_password(&request.new_password)?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query(
        r#"UPDATE users SET password_hash=$2, must_change_password=false,
             password_changed_at=now(), updated_at=now() WHERE id=$1"#,
    )
    .bind(principal.user_id)
    .bind(&new_hash)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE sessions SET revoked_at=now() WHERE user_id=$1 AND id<>$2 AND revoked_at IS NULL",
    )
    .bind(principal.user_id)
    .bind(principal.session_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.password.changed",
        principal.user_id,
        json!({"otherSessionsRevoked": true}),
        &headers,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn recovery_key_state(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    if !principal.is_super_admin() {
        return Err(ApiError::forbidden(
            "Only a Super Admin holds an administrator recovery key.",
        ));
    }
    let row = sqlx::query(
        "SELECT recovery_key_origin, recovery_key_confirmed_at IS NOT NULL AS confirmed FROM users WHERE id=$1",
    )
    .bind(principal.user_id)
    .fetch_one(&state.pool)
    .await?;
    let origin: Option<String> = row.try_get("recovery_key_origin")?;
    let confirmed: bool = row.try_get("confirmed")?;
    let Some(origin) = origin else {
        return Err(ApiError::not_found(
            "No administrator recovery key is configured for this account.",
        ));
    };
    let dir = state
        .config
        .admin_recovery_key_dir
        .as_deref()
        .ok_or_else(|| {
            ApiError::service_unavailable("Administrator recovery keys are not configured.")
        })?;
    let recovery_key = if origin == crate::services::recovery_key::GENERATED_ORIGIN && !confirmed {
        crate::services::recovery_key::read_existing(dir)?
    } else {
        None
    };
    Ok(sensitive_json(RecoveryKeyState {
        origin,
        confirmed,
        recovery_key,
    }))
}

pub async fn confirm_recovery_key(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    if !principal.is_super_admin() {
        return Err(ApiError::forbidden(
            "Only a Super Admin holds an administrator recovery key.",
        ));
    }
    let updated = sqlx::query(
        r#"UPDATE users SET recovery_key_confirmed_at=now(), updated_at=now()
           WHERE id=$1 AND recovery_key_hash IS NOT NULL AND recovery_key_confirmed_at IS NULL"#,
    )
    .bind(principal.user_id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if updated == 0 {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND recovery_key_hash IS NOT NULL)",
        )
        .bind(principal.user_id)
        .fetch_one(&state.pool)
        .await?;
        if !exists {
            return Err(ApiError::not_found(
                "No administrator recovery key is configured for this account.",
            ));
        }
    }
    record_auth_audit(
        &state,
        &principal.email,
        "auth.recovery_key.confirmed",
        principal.user_id,
        json!({"confirmed": true}),
        &headers,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn recover_with_key(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<RecoveryRequest>,
) -> Result<Response, ApiError> {
    validate_strong_password(&request.new_password)?;
    let email = request.email.trim().to_ascii_lowercase();
    let rate_keys = rate_limit_keys("recovery", &request_source(&state, &headers, peer), &email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    let row = sqlx::query("SELECT id, recovery_key_hash FROM users WHERE lower(email)=lower($1)")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    let valid = row.as_ref().is_some_and(|row| {
        row.try_get::<Option<String>, _>("recovery_key_hash")
            .ok()
            .flatten()
            .is_some_and(|hash| verify_secret(&hash, request.recovery_key.trim()))
    });
    if !valid {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::unauthorized("The recovery key is invalid."));
    }
    let user_id: Uuid = row.expect("validated row exists").try_get("id")?;
    let dir = state
        .config
        .admin_recovery_key_dir
        .as_deref()
        .ok_or_else(|| {
            ApiError::service_unavailable("Administrator recovery keys are not configured.")
        })?;
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let new_hash = hash_password(&request.new_password)?;
    let rotated = crate::services::recovery_key::rotate(dir)?;
    let rotated_hash = hash_secret(&rotated.key)?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query(
        r#"UPDATE users SET password_hash=$2, must_change_password=false,
             password_changed_at=now(), totp_secret_ciphertext=NULL, totp_confirmed_at=NULL,
             recovery_key_hash=$3, recovery_key_origin='generated',
             recovery_key_confirmed_at=NULL, recovery_key_rotated_at=now(), updated_at=now()
           WHERE id=$1"#,
    )
    .bind(user_id)
    .bind(&new_hash)
    .bind(&rotated_hash)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    record_auth_audit(
        &state,
        &email,
        "auth.recovery_key.used",
        user_id,
        json!({"sessionsRevoked": true, "totpCleared": true, "keyRotated": true}),
        &headers,
    )
    .await?;
    Ok(sensitive_json(RecoveryKeyRotationResult {
        recovery_key: rotated.key,
        rotated_at: Utc::now(),
    }))
}
