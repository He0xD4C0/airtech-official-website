pub async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, ApiError> {
    let token = cookie_value(headers, SESSION_COOKIE)
        .ok_or_else(|| ApiError::unauthorized("An active admin session is required."))?;
    let supplied_hash = token_hash(&token);

    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT session.id AS session_id, session.user_id, session.csrf_hash,
                      session.expires_at, session.last_seen_at, user_account.display_name, user_account.email,
                      user_account.status, user_account.totp_confirmed_at
               FROM sessions AS session
               JOIN users AS user_account ON user_account.id = session.user_id
               WHERE session.token_hash = $1 AND session.revoked_at IS NULL"#,
        )
        .bind(&supplied_hash)
        .fetch_optional(pool)
        .await?;
        let row = row.ok_or_else(|| ApiError::unauthorized("The admin session is invalid."))?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        let last_seen_at: DateTime<Utc> = row.try_get("last_seen_at")?;
        let status: String = row.try_get("status")?;
        let now = Utc::now();
        if expires_at <= now
            || last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) <= now
            || status != "active"
        {
            sqlx::query("UPDATE sessions SET revoked_at=COALESCE(revoked_at, now()) WHERE id=$1")
                .bind(row.try_get::<Uuid, _>("session_id")?)
                .execute(pool)
                .await?;
            return Err(ApiError::unauthorized("The admin session has expired."));
        }
        let user_id: Uuid = row.try_get("user_id")?;
        let (role, permissions) = load_roles_and_permissions(pool, user_id).await?;
        sqlx::query("UPDATE sessions SET last_seen_at=now() WHERE id=$1")
            .bind(row.try_get::<Uuid, _>("session_id")?)
            .execute(pool)
            .await?;
        return Ok(AdminPrincipal {
            user_id,
            display_name: row.try_get("display_name")?,
            email: row.try_get("email")?,
            role,
            permissions,
            session_id: row.try_get("session_id")?,
            session_token_hash: supplied_hash,
            csrf_hash: row.try_get("csrf_hash")?,
            totp_enabled: row
                .try_get::<Option<DateTime<Utc>>, _>("totp_confirmed_at")?
                .is_some(),
        });
    }

    let (stored_session, user) = {
        let data = state.data.read().await;
        let now = Utc::now();
        let stored_session = data
            .admin_sessions
            .values()
            .find(|session| {
                !session.revoked
                    && session.expires_at > now
                    && session.last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) > now
                    && constant_time_equal(&session.token_hash, &supplied_hash)
            })
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("The admin session is invalid or expired."))?;
        let user = data
            .admin_users
            .get(&stored_session.user_id)
            .filter(|user| user.active)
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("The admin account is disabled."))?;
        (stored_session, user)
    };
    if let Some(session) = state
        .data
        .write()
        .await
        .admin_sessions
        .get_mut(&stored_session.id)
    {
        session.last_seen_at = Utc::now();
    }
    Ok(AdminPrincipal {
        user_id: user.id,
        display_name: user.display_name,
        email: user.email,
        role: user.role,
        permissions: user.permissions,
        session_id: stored_session.id,
        session_token_hash: stored_session.token_hash,
        csrf_hash: stored_session.csrf_hash,
        totp_enabled: user.totp_enabled,
    })
}

/// Revalidate the identity and authorization bound into a signed preview token.
///
/// The token is only a short-lived transport credential: disabling the user,
/// revoking or expiring the issuing session, or removing `content.read` must
/// invalidate the preview immediately. Production always takes the PostgreSQL
/// branch because the API binary refuses to start without `DATABASE_URL`.
pub async fn preview_session_is_authorized(
    state: &AppState,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<bool, ApiError> {
    if user_id.is_nil() || session_id.is_nil() {
        return Ok(false);
    }
    if let Some(pool) = &state.pool {
        return Ok(sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                   FROM sessions AS session
                   JOIN users AS user_account ON user_account.id=session.user_id
                   JOIN user_roles AS assignment ON assignment.user_id=user_account.id
                   JOIN role_permissions AS role_permission
                     ON role_permission.role_id=assignment.role_id
                   WHERE session.id=$1
                     AND session.user_id=$2
                     AND session.revoked_at IS NULL
                     AND session.expires_at > now()
                     AND session.last_seen_at + ($3::bigint * interval '1 minute') > now()
                     AND user_account.status='active'
                     AND user_account.totp_confirmed_at IS NOT NULL
                     AND role_permission.permission_key='content.read'
               )"#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(SESSION_IDLE_MINUTES)
        .fetch_one(pool)
        .await?);
    }

    let data = state.data.read().await;
    let now = Utc::now();
    let Some(session) = data.admin_sessions.get(&session_id).filter(|session| {
        session.user_id == user_id
            && !session.revoked
            && session.expires_at > now
            && session.last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) > now
    }) else {
        return Ok(false);
    };
    Ok(data.admin_users.get(&session.user_id).is_some_and(|user| {
        user.active
            && user.totp_enabled
            && user.permissions.iter().any(|value| value == "content.read")
    }))
}

async fn authenticate_with_csrf(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, ApiError> {
    let principal = authenticate(state, headers).await?;
    verify_csrf(headers, &principal)?;
    Ok(principal)
}
