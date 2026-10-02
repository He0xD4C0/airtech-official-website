use super::*;

pub(super) fn totp_encryption_key(
    state: &AppState,
) -> Result<&crate::config::TotpEncryptionKey, ApiError> {
    state.config.totp_encryption_key.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(
            "TOTP is unavailable until AIRTEK_TOTP_ENCRYPTION_KEY is configured.",
        )
    })
}

pub(super) async fn stored_totp_ciphertext(
    state: &AppState,
    user_id: Uuid,
) -> Result<Vec<u8>, ApiError> {
    sqlx::query_scalar::<_, Option<Vec<u8>>>("SELECT totp_secret_ciphertext FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .flatten()
        .ok_or_else(|| ApiError::bad_request("Start TOTP enrollment before confirming it."))
}

pub(super) async fn verify_pending_totp(
    state: &AppState,
    user_id: Uuid,
    account: &str,
    code: &str,
) -> Result<bool, ApiError> {
    let ciphertext = stored_totp_ciphertext(state, user_id).await?;
    EnrollmentSecret::open(&ciphertext, totp_encryption_key(state)?, user_id)?
        .verify_current(code, account)
}

pub async fn verify_totp_reauthentication(
    state: &AppState,
    principal: &AdminPrincipal,
    code: &str,
) -> Result<(), ApiError> {
    if !principal.totp_enabled {
        return Err(ApiError::forbidden(
            "This account must enable TOTP before running high-risk operations.",
        ));
    }
    let rate_keys = rate_limit_keys("totp-reauth", "authenticated-session", &principal.email);
    check_auth_rate_limits(state, &rate_keys).await?;
    if !verify_principal_totp(state, principal, code.trim()).await? {
        record_auth_failures(state, &rate_keys).await?;
        return Err(ApiError::forbidden("A valid X-TOTP-Code is required."));
    }
    clear_auth_rate_limits(state, &rate_keys).await?;
    Ok(())
}

pub(super) async fn verify_principal_totp(
    state: &AppState,
    principal: &AdminPrincipal,
    code: &str,
) -> Result<bool, ApiError> {
    verify_pending_totp(state, principal.user_id, &principal.email, code).await
}

pub(super) async fn verify_login_second_factor(
    state: &AppState,
    user: &StoredUser,
    code: &str,
) -> Result<bool, ApiError> {
    if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(false);
    }
    let ciphertext = user.totp_secret_ciphertext.as_deref().ok_or_else(|| {
        ApiError::service_unavailable("The confirmed TOTP secret is unavailable.")
    })?;
    let valid = EnrollmentSecret::open(ciphertext, totp_encryption_key(state)?, user.id)?
        .verify_current(code, &user.email)?;
    Ok(valid)
}

pub(super) async fn persist_confirmed_totp(
    state: &AppState,
    user_id: Uuid,
) -> Result<(), ApiError> {
    let pool = &state.pool;
    let result = sqlx::query(
        r#"UPDATE users SET totp_confirmed_at=now(), updated_at=now()
               WHERE id=$1 AND totp_confirmed_at IS NULL AND totp_secret_ciphertext IS NOT NULL"#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "TOTP enrollment was already confirmed or is unavailable.",
        ));
    }
    Ok(())
}

pub(super) async fn record_auth_audit(
    state: &AppState,
    actor: &str,
    action: &str,
    entity_id: Uuid,
    after: serde_json::Value,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    state
        .persist_audit(AuditEvent {
            id: Uuid::new_v4(),
            actor: actor.into(),
            action: action.into(),
            entity_type: if action.contains("session") {
                "session".into()
            } else {
                "user".into()
            },
            entity_id: Some(entity_id),
            before: None,
            after: Some(after),
            reason: None,
            current_version: None,
            request_id,
            occurred_at: Utc::now(),
        })
        .await
}
