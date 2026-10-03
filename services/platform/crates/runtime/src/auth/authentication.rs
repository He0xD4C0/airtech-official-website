use super::*;

pub async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, ApiError> {
    let token = cookie_value(headers, SESSION_COOKIE)
        .ok_or_else(|| ApiError::unauthorized("An active admin session is required."))?;
    let supplied_hash = token_hash(&token);

    let pool = &state.pool;
    let row = sqlx::query(
            r#"SELECT session.id AS session_id, session.user_id, session.csrf_hash,
                      session.expires_at, session.last_seen_at, user_account.display_name, user_account.email,
                      user_account.status, user_account.totp_confirmed_at,
                      user_account.must_change_password, user_account.phone_verified_at,
                      user_account.recovery_key_hash, user_account.recovery_key_confirmed_at
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
    let (role, role_keys, permissions) = load_roles_and_permissions(pool, user_id).await?;
    sqlx::query("UPDATE sessions SET last_seen_at=now() WHERE id=$1")
        .bind(row.try_get::<Uuid, _>("session_id")?)
        .execute(pool)
        .await?;
    let recovery_key_hash: Option<String> = row.try_get("recovery_key_hash")?;
    let recovery_key_confirmed_at: Option<DateTime<Utc>> =
        row.try_get("recovery_key_confirmed_at")?;
    Ok(AdminPrincipal {
        user_id,
        display_name: row.try_get("display_name")?,
        email: row.try_get("email")?,
        role,
        role_keys,
        permissions,
        session_id: row.try_get("session_id")?,
        session_token_hash: supplied_hash,
        csrf_hash: row.try_get("csrf_hash")?,
        totp_enabled: row
            .try_get::<Option<DateTime<Utc>>, _>("totp_confirmed_at")?
            .is_some(),
        must_change_password: row.try_get("must_change_password")?,
        must_confirm_recovery_key: recovery_key_hash.is_some()
            && recovery_key_confirmed_at.is_none(),
        phone_verified: row
            .try_get::<Option<DateTime<Utc>>, _>("phone_verified_at")?
            .is_some(),
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
    Ok(sqlx::query_scalar::<_, bool>(
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
                     AND role_permission.permission_key='content.read'
               )"#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(SESSION_IDLE_MINUTES)
    .fetch_one(&state.pool)
    .await?)
}

pub(super) async fn authenticate_with_csrf(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, ApiError> {
    let principal = authenticate(state, headers).await?;
    verify_csrf(headers, &principal)?;
    Ok(principal)
}
