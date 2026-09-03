async fn login(
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
    let second_factor = if user.totp_enabled {
        let Some(code) = request
            .otp
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
        else {
            record_auth_failures(&state, &rate_keys).await?;
            return Err(ApiError::unauthorized(
                "A valid TOTP or recovery code is required.",
            ));
        };
        let Some(method) = verify_login_second_factor(&state, &user, code).await? else {
            record_auth_failures(&state, &rate_keys).await?;
            return Err(ApiError::unauthorized(
                "A valid TOTP or recovery code is required.",
            ));
        };
        Some(method)
    } else {
        None
    };
    let issue = create_session(&state, user).await?;
    record_auth_audit(
        &state,
        &issue.principal.email,
        "auth.login",
        issue.principal.user_id,
        json!({"secondFactor": second_factor.map(SecondFactorMethod::label).unwrap_or("none")}),
        &headers,
    )
    .await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    Ok(session_response(&state, issue, StatusCode::OK))
}

async fn session(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let csrf_token = random_token();
    rotate_csrf(&state, &principal, &csrf_token).await?;
    let refreshed = AdminPrincipal {
        csrf_hash: token_hash(&csrf_token),
        ..principal
    };
    Ok(session_refresh_response(&state, refreshed, csrf_token))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
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

async fn start_totp_enrollment(
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
    if let Some(pool) = &state.pool {
        let result = sqlx::query(
            "UPDATE users SET totp_secret_ciphertext=$1, updated_at=now() WHERE id=$2 AND totp_confirmed_at IS NULL",
        )
        .bind(&ciphertext)
        .bind(principal.user_id)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "TOTP is already enabled for this account.",
            ));
        }
    } else {
        let mut data = state.data.write().await;
        let user = data
            .admin_users
            .get_mut(&principal.user_id)
            .ok_or_else(|| ApiError::unauthorized("The admin account is unavailable."))?;
        if user.totp_enabled {
            return Err(ApiError::conflict(
                "TOTP is already enabled for this account.",
            ));
        }
        user.totp_secret_ciphertext = Some(ciphertext);
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

async fn confirm_totp_enrollment(
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
    let (codes, hashes) = new_recovery_code_set()?;
    persist_confirmed_totp(&state, principal.user_id, &hashes).await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.totp.enabled",
        principal.user_id,
        json!({"totpEnabled": true, "recoveryCodeCount": codes.len()}),
        &headers,
    )
    .await?;
    Ok(sensitive_json(RecoveryCodeSet {
        recovery_codes: codes,
        generated_at: Utc::now(),
    }))
}

async fn regenerate_recovery_codes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TotpCodeRequest>,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    if !principal.totp_enabled {
        return Err(ApiError::forbidden(
            "TOTP must be enabled before recovery codes can be regenerated.",
        ));
    }
    let rate_keys = rate_limit_keys("totp-regenerate", "authenticated-session", &principal.email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    if !verify_principal_totp(&state, &principal, request.code.trim()).await? {
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
    let (codes, hashes) = new_recovery_code_set()?;
    replace_recovery_codes(&state, principal.user_id, &hashes).await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.recovery_codes.regenerated",
        principal.user_id,
        json!({"recoveryCodeCount": codes.len()}),
        &headers,
    )
    .await?;
    Ok(sensitive_json(RecoveryCodeSet {
        recovery_codes: codes,
        generated_at: Utc::now(),
    }))
}

async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let now = Utc::now();
    let mut sessions = if let Some(pool) = &state.pool {
        sqlx::query(
            r#"SELECT id, created_at, last_seen_at, expires_at
               FROM sessions WHERE user_id=$1 AND revoked_at IS NULL
               AND expires_at > now() AND last_seen_at > now() - interval '30 minutes'
               ORDER BY last_seen_at DESC"#,
        )
        .bind(principal.user_id)
        .fetch_all(pool)
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
        .collect::<Result<Vec<_>, sqlx::Error>>()?
    } else {
        state
            .data
            .read()
            .await
            .admin_sessions
            .values()
            .filter(|session| {
                session.user_id == principal.user_id
                    && !session.revoked
                    && session.expires_at > now
                    && session.last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) > now
            })
            .map(|session| SessionSummary {
                id: session.id,
                current: session.id == principal.session_id,
                created_at: session.created_at,
                last_seen_at: session.last_seen_at,
                expires_at: session.expires_at,
            })
            .collect()
    };
    sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seen_at));
    Ok(sensitive_json(sessions))
}

async fn revoke_session_by_id(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    let revoked = if let Some(pool) = &state.pool {
        sqlx::query(
            "UPDATE sessions SET revoked_at=now() WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(principal.user_id)
        .execute(pool)
        .await?
        .rows_affected()
            == 1
    } else {
        let mut data = state.data.write().await;
        data.admin_sessions
            .get_mut(&session_id)
            .filter(|session| session.user_id == principal.user_id && !session.revoked)
            .map(|session| session.revoked = true)
            .is_some()
    };
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
