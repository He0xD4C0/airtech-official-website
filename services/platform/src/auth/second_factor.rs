fn totp_encryption_key(state: &AppState) -> Result<&crate::config::TotpEncryptionKey, ApiError> {
    state.config.totp_encryption_key.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(
            "TOTP is unavailable until AIRTEK_TOTP_ENCRYPTION_KEY is configured.",
        )
    })
}

async fn stored_totp_ciphertext(state: &AppState, user_id: Uuid) -> Result<Vec<u8>, ApiError> {
    if let Some(pool) = &state.pool {
        return sqlx::query_scalar::<_, Option<Vec<u8>>>(
            "SELECT totp_secret_ciphertext FROM users WHERE id=$1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .flatten()
        .ok_or_else(|| ApiError::bad_request("Start TOTP enrollment before confirming it."));
    }
    state
        .data
        .read()
        .await
        .admin_users
        .get(&user_id)
        .and_then(|user| user.totp_secret_ciphertext.clone())
        .ok_or_else(|| ApiError::bad_request("Start TOTP enrollment before confirming it."))
}

async fn verify_pending_totp(
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

async fn verify_principal_totp(
    state: &AppState,
    principal: &AdminPrincipal,
    code: &str,
) -> Result<bool, ApiError> {
    verify_pending_totp(state, principal.user_id, &principal.email, code).await
}

async fn verify_login_second_factor(
    state: &AppState,
    user: &StoredUser,
    code: &str,
) -> Result<Option<SecondFactorMethod>, ApiError> {
    if code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit()) {
        let ciphertext = user.totp_secret_ciphertext.as_deref().ok_or_else(|| {
            ApiError::service_unavailable("The confirmed TOTP secret is unavailable.")
        })?;
        let valid = EnrollmentSecret::open(ciphertext, totp_encryption_key(state)?, user.id)?
            .verify_current(code, &user.email)?;
        return Ok(valid.then_some(SecondFactorMethod::Totp));
    }
    let Some(normalized) = normalize_recovery_code(code) else {
        return Ok(None);
    };
    Ok(consume_recovery_code(state, user.id, &normalized)
        .await?
        .then_some(SecondFactorMethod::RecoveryCode))
}

async fn consume_recovery_code(
    state: &AppState,
    user_id: Uuid,
    normalized: &str,
) -> Result<bool, ApiError> {
    if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            "SELECT code_hash FROM recovery_codes WHERE user_id=$1 AND used_at IS NULL",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        for row in rows {
            let encoded = row.try_get::<Vec<u8>, _>("code_hash")?;
            let Ok(encoded) = String::from_utf8(encoded) else {
                continue;
            };
            if verify_password(&encoded, normalized) {
                let consumed = sqlx::query(
                    "UPDATE recovery_codes SET used_at=now() WHERE user_id=$1 AND code_hash=$2 AND used_at IS NULL",
                )
                .bind(user_id)
                .bind(encoded.as_bytes())
                .execute(pool)
                .await?
                .rows_affected()
                    == 1;
                return Ok(consumed);
            }
        }
        return Ok(false);
    }
    let mut data = state.data.write().await;
    let Some(codes) = data.recovery_codes.get_mut(&user_id) else {
        return Ok(false);
    };
    for code in codes.iter_mut().filter(|code| !code.used) {
        if verify_password(&code.hash, normalized) {
            code.used = true;
            return Ok(true);
        }
    }
    Ok(false)
}

fn new_recovery_code_set() -> Result<(Vec<String>, Vec<String>), ApiError> {
    let codes = generate_recovery_codes()?;
    let hashes = codes
        .iter()
        .map(|code| {
            normalize_recovery_code(code)
                .ok_or_else(|| ApiError::internal("Generated recovery code is invalid."))
                .and_then(|normalized| hash_password(&normalized))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((codes, hashes))
}

async fn persist_confirmed_totp(
    state: &AppState,
    user_id: Uuid,
    hashes: &[String],
) -> Result<(), ApiError> {
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE users SET totp_confirmed_at=now(), updated_at=now()
               WHERE id=$1 AND totp_confirmed_at IS NULL AND totp_secret_ciphertext IS NOT NULL"#,
        )
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "TOTP enrollment was already confirmed or is unavailable.",
            ));
        }
        replace_recovery_codes_transaction(&mut transaction, user_id, hashes).await?;
        transaction.commit().await?;
        return Ok(());
    }
    let mut data = state.data.write().await;
    let user = data
        .admin_users
        .get_mut(&user_id)
        .ok_or_else(|| ApiError::unauthorized("The admin account is unavailable."))?;
    if user.totp_enabled || user.totp_secret_ciphertext.is_none() {
        return Err(ApiError::conflict(
            "TOTP enrollment was already confirmed or is unavailable.",
        ));
    }
    user.totp_enabled = true;
    data.recovery_codes.insert(
        user_id,
        hashes
            .iter()
            .map(|hash| StoredRecoveryCode {
                hash: hash.clone(),
                used: false,
            })
            .collect(),
    );
    Ok(())
}

async fn replace_recovery_codes(
    state: &AppState,
    user_id: Uuid,
    hashes: &[String],
) -> Result<(), ApiError> {
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        replace_recovery_codes_transaction(&mut transaction, user_id, hashes).await?;
        transaction.commit().await?;
    } else {
        state.data.write().await.recovery_codes.insert(
            user_id,
            hashes
                .iter()
                .map(|hash| StoredRecoveryCode {
                    hash: hash.clone(),
                    used: false,
                })
                .collect(),
        );
    }
    Ok(())
}

async fn replace_recovery_codes_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    hashes: &[String],
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM recovery_codes WHERE user_id=$1")
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;
    for hash in hashes {
        sqlx::query("INSERT INTO recovery_codes (user_id, code_hash) VALUES ($1,$2)")
            .bind(user_id)
            .bind(hash.as_bytes())
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

async fn record_auth_audit(
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
            request_id,
            occurred_at: Utc::now(),
        })
        .await
}
