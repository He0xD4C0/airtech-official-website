use super::*;

pub async fn login(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let email = request.email.trim().to_ascii_lowercase();
    let rate_keys = rate_limit_keys("login", &request_source(&state, &headers, peer), &email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    let user = find_user_by_email(&state, &email).await?;
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let valid = if let Some(user) = &user {
        user.active && verify_password(&user.password_hash, &request.password)
    } else {
        // Make unknown-user login perform Argon2 work too. This is not intended
        // to be perfectly identical timing, but avoids the trivial fast path.
        let dummy = DUMMY_PASSWORD_HASH.get_or_init(|| {
            hash_password("invalid-login-password")
                .expect("the fixed Argon2 dummy password must be hashable")
        });
        let _ = verify_password(dummy, &request.password);
        false
    };
    if !valid {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::unauthorized("Email or password is incorrect."));
    }
    let user = user.expect("valid login has a user");
    let totp_satisfied = if user.totp_enabled {
        let Some(code) = request
            .otp
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
        else {
            record_auth_failures(&state, &rate_keys).await?;
            return Err(ApiError::unauthorized("A valid TOTP code is required."));
        };
        if !verify_login_second_factor(&state, &user, code).await? {
            record_auth_failures(&state, &rate_keys).await?;
            return Err(ApiError::unauthorized("A valid TOTP code is required."));
        }
        true
    } else {
        false
    };
    let issue = create_session(&state, user).await?;
    record_auth_audit(
        &state,
        &issue.principal.email,
        "auth.login",
        issue.principal.user_id,
        json!({"secondFactor": if totp_satisfied { "totp" } else { "none" }}),
        &headers,
    )
    .await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    Ok(session_response(&state, issue, StatusCode::OK))
}

pub async fn session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let csrf_token = random_token();
    rotate_csrf(&state, &principal, &csrf_token).await?;
    let refreshed = AdminPrincipal {
        csrf_hash: token_hash(&csrf_token),
        ..principal
    };
    Ok(session_refresh_response(&state, refreshed, csrf_token))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if cookie_value(&headers, SESSION_COOKIE).is_some() {
        match authenticate(&state, &headers).await {
            Ok(principal) => {
                verify_csrf(&headers, &principal)?;
                revoke_session(&state, &principal).await?;
                record_auth_audit(
                    &state,
                    &principal.email,
                    "auth.logout",
                    principal.session_id,
                    json!({"currentSession": true}),
                    &headers,
                )
                .await?;
            }
            Err(error) if error.status() == StatusCode::UNAUTHORIZED => {}
            Err(error) => return Err(error),
        }
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    append_clear_cookies(&state, response.headers_mut());
    Ok(response)
}

pub async fn start_totp_enrollment(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    if principal.totp_enabled {
        return Err(ApiError::conflict(
            "TOTP is already enabled for this account.",
        ));
    }
    let key = totp_encryption_key(&state)?;
    let secret = EnrollmentSecret::generate()?;
    let ciphertext = secret.seal(key, principal.user_id)?;
    let result = sqlx::query(
            "UPDATE users SET totp_secret_ciphertext=$1, updated_at=now() WHERE id=$2 AND totp_confirmed_at IS NULL",
        )
        .bind(&ciphertext)
        .bind(principal.user_id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "TOTP is already enabled for this account.",
        ));
    }
    record_auth_audit(
        &state,
        &principal.email,
        "auth.totp.enrollment_started",
        principal.user_id,
        json!({"totpEnabled": false}),
        &headers,
    )
    .await?;
    Ok(sensitive_json(TotpEnrollment {
        secret: secret.base32(),
        otp_auth_uri: secret.provisioning_uri(&principal.email)?,
        algorithm: "SHA1",
        digits: 6,
        period_seconds: 30,
    }))
}

pub async fn confirm_totp_enrollment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TotpCodeRequest>,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    if principal.totp_enabled {
        return Err(ApiError::conflict(
            "TOTP is already enabled for this account.",
        ));
    }
    let rate_keys = rate_limit_keys("totp-confirm", "authenticated-session", &principal.email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    if !verify_pending_totp(
        &state,
        principal.user_id,
        &principal.email,
        request.code.trim(),
    )
    .await?
    {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::unauthorized(
            "The TOTP code is invalid or expired.",
        ));
    }
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    persist_confirmed_totp(&state, principal.user_id).await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.totp.enabled",
        principal.user_id,
        json!({"totpEnabled": true}),
        &headers,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let mut sessions = sqlx::query(
        r#"SELECT id, created_at, last_seen_at, expires_at
               FROM sessions WHERE user_id=$1 AND revoked_at IS NULL
               AND expires_at > now() AND last_seen_at > now() - interval '30 minutes'
               ORDER BY last_seen_at DESC"#,
    )
    .bind(principal.user_id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|row| {
        let id: Uuid = row.try_get("id")?;
        Ok(SessionSummary {
            id,
            current: id == principal.session_id,
            created_at: row.try_get("created_at")?,
            last_seen_at: row.try_get("last_seen_at")?,
            expires_at: row.try_get("expires_at")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seen_at));
    Ok(sensitive_json(sessions))
}

pub async fn revoke_session_by_id(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    let revoked = sqlx::query(
        "UPDATE sessions SET revoked_at=now() WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL",
    )
    .bind(session_id)
    .bind(principal.user_id)
    .execute(&state.pool)
    .await?
    .rows_affected()
        == 1;
    if !revoked {
        return Err(ApiError::not_found("The active session was not found."));
    }
    record_auth_audit(
        &state,
        &principal.email,
        "auth.session.revoked",
        session_id,
        json!({"currentSession": session_id == principal.session_id}),
        &headers,
    )
    .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    if session_id == principal.session_id {
        append_clear_cookies(&state, response.headers_mut());
    }
    Ok(response)
}
