fn rate_limit_key(scope: &str, source: &str, account: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{scope}|{}|{}",
                source.trim(),
                account.trim().to_ascii_lowercase()
            )
            .as_bytes()
        )
    )
}

fn rate_limit_keys(scope: &str, source: &str, account: &str) -> Vec<String> {
    vec![
        rate_limit_key(scope, "all-sources", account),
        rate_limit_key(scope, source, account),
    ]
}

async fn check_auth_rate_limits(state: &AppState, keys: &[String]) -> Result<(), ApiError> {
    for key in keys {
        check_auth_rate_limit(state, key).await?;
    }
    Ok(())
}

async fn record_auth_failures(state: &AppState, keys: &[String]) -> Result<(), ApiError> {
    for key in keys {
        record_auth_failure(state, key).await?;
    }
    Ok(())
}

async fn clear_auth_rate_limits(state: &AppState, keys: &[String]) -> Result<(), ApiError> {
    for key in keys {
        clear_auth_rate_limit(state, key).await?;
    }
    Ok(())
}

async fn check_auth_rate_limit(state: &AppState, key: &str) -> Result<(), ApiError> {
    let now = Utc::now();
    if let Some(pool) = &state.pool {
        let blocked_until = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT blocked_until FROM auth_rate_limits WHERE key_hash=$1",
        )
        .bind(key)
        .fetch_optional(pool)
        .await?
        .flatten();
        if blocked_until.is_some_and(|until| until > now) {
            return Err(ApiError::too_many_requests(
                "Too many authentication attempts. Try again later.",
            ));
        }
        return Ok(());
    }
    if state
        .data
        .read()
        .await
        .auth_rate_limits
        .get(key)
        .and_then(|record| record.blocked_until)
        .is_some_and(|until| until > now)
    {
        return Err(ApiError::too_many_requests(
            "Too many authentication attempts. Try again later.",
        ));
    }
    Ok(())
}

async fn record_auth_failure(state: &AppState, key: &str) -> Result<(), ApiError> {
    let now = Utc::now();
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(key)
            .execute(&mut *transaction)
            .await?;
        let row = sqlx::query(
            "SELECT attempts, window_started_at FROM auth_rate_limits WHERE key_hash=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *transaction)
        .await?;
        let (attempts, window_started_at) = if let Some(row) = row {
            let started: DateTime<Utc> = row.try_get("window_started_at")?;
            let attempts: i32 = row.try_get("attempts")?;
            if started + Duration::minutes(RATE_WINDOW_MINUTES) <= now {
                (1_u32, now)
            } else {
                (attempts.max(0) as u32 + 1, started)
            }
        } else {
            (1, now)
        };
        let blocked_until =
            (attempts >= RATE_MAX_FAILURES).then(|| now + Duration::minutes(RATE_BLOCK_MINUTES));
        sqlx::query(
            r#"INSERT INTO auth_rate_limits
               (key_hash, attempts, window_started_at, blocked_until, updated_at)
               VALUES ($1,$2,$3,$4,$5)
               ON CONFLICT (key_hash) DO UPDATE SET attempts=EXCLUDED.attempts,
               window_started_at=EXCLUDED.window_started_at,
               blocked_until=EXCLUDED.blocked_until, updated_at=EXCLUDED.updated_at"#,
        )
        .bind(key)
        .bind(attempts as i32)
        .bind(window_started_at)
        .bind(blocked_until)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        return Ok(());
    }

    let mut data = state.data.write().await;
    data.auth_rate_limits.retain(|_, record| {
        record.updated_at + Duration::minutes(RATE_WINDOW_MINUTES + RATE_BLOCK_MINUTES) > now
    });
    if !data.auth_rate_limits.contains_key(key)
        && data.auth_rate_limits.len() >= RATE_MAX_MEMORY_KEYS
    {
        if let Some(oldest) = data
            .auth_rate_limits
            .iter()
            .min_by_key(|(_, record)| record.updated_at)
            .map(|(key, _)| key.clone())
        {
            data.auth_rate_limits.remove(&oldest);
        }
    }
    let record = data
        .auth_rate_limits
        .entry(key.to_owned())
        .or_insert(AuthRateLimit {
            attempts: 0,
            window_started_at: now,
            blocked_until: None,
            updated_at: now,
        });
    if record.window_started_at + Duration::minutes(RATE_WINDOW_MINUTES) <= now {
        record.attempts = 0;
        record.window_started_at = now;
        record.blocked_until = None;
    }
    record.attempts += 1;
    record.updated_at = now;
    if record.attempts >= RATE_MAX_FAILURES {
        record.blocked_until = Some(now + Duration::minutes(RATE_BLOCK_MINUTES));
    }
    Ok(())
}

async fn clear_auth_rate_limit(state: &AppState, key: &str) -> Result<(), ApiError> {
    if let Some(pool) = &state.pool {
        sqlx::query("DELETE FROM auth_rate_limits WHERE key_hash=$1")
            .bind(key)
            .execute(pool)
            .await?;
    } else {
        state.data.write().await.auth_rate_limits.remove(key);
    }
    Ok(())
}

fn valid_email(value: &str) -> bool {
    value.len() <= 254
        && !value.contains(':')
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
}
