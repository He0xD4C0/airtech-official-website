use super::*;

pub(super) fn rate_limit_key(scope: &str, source: &str, account: &str) -> String {
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

pub(super) fn rate_limit_keys(scope: &str, source: &str, account: &str) -> Vec<String> {
    vec![
        rate_limit_key(scope, "all-sources", account),
        rate_limit_key(scope, source, account),
    ]
}

pub(super) async fn check_auth_rate_limits(
    state: &AppState,
    keys: &[String],
) -> Result<(), ApiError> {
    for key in keys {
        check_auth_rate_limit(state, key).await?;
    }
    Ok(())
}

pub(super) async fn record_auth_failures(
    state: &AppState,
    keys: &[String],
) -> Result<(), ApiError> {
    for key in keys {
        record_auth_failure(state, key).await?;
    }
    Ok(())
}

pub(super) async fn clear_auth_rate_limits(
    state: &AppState,
    keys: &[String],
) -> Result<(), ApiError> {
    for key in keys {
        clear_auth_rate_limit(state, key).await?;
    }
    Ok(())
}

pub(super) async fn check_auth_rate_limit(state: &AppState, key: &str) -> Result<(), ApiError> {
    let now = Utc::now();
    let blocked_until = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        "SELECT blocked_until FROM auth_rate_limits WHERE key_hash=$1",
    )
    .bind(key)
    .fetch_optional(&state.pool)
    .await?
    .flatten();
    if blocked_until.is_some_and(|until| until > now) {
        return Err(ApiError::too_many_requests(
            "Too many authentication attempts. Try again later.",
        ));
    }
    Ok(())
}

pub(super) async fn record_auth_failure(state: &AppState, key: &str) -> Result<(), ApiError> {
    let now = Utc::now();
    let mut transaction = state.pool.begin().await?;
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
    Ok(())
}

pub(super) async fn clear_auth_rate_limit(state: &AppState, key: &str) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM auth_rate_limits WHERE key_hash=$1")
        .bind(key)
        .execute(&state.pool)
        .await?;
    Ok(())
}

pub(super) fn valid_email(value: &str) -> bool {
    value.len() <= 254
        && !value.contains(':')
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
}
