use super::*;

pub fn verify_csrf(headers: &HeaderMap, principal: &AdminPrincipal) -> Result<(), ApiError> {
    let token = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::forbidden("X-CSRF-Token is required for this request."))?;
    let supplied_hash = token_hash(token);
    if !constant_time_equal(&supplied_hash, &principal.csrf_hash) {
        return Err(ApiError::forbidden("The CSRF token is invalid or expired."));
    }
    Ok(())
}

pub(super) fn validate_strong_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 12
        || password.len() > 256
        || !password.chars().any(|value| value.is_ascii_alphabetic())
        || !password.chars().any(|value| value.is_ascii_digit())
    {
        return Err(ApiError::bad_request(
            "Password must contain 12 to 256 characters, including a letter and a digit.",
        ));
    }
    Ok(())
}

pub(super) async fn find_user_by_email(
    state: &AppState,
    email: &str,
) -> Result<Option<StoredUser>, ApiError> {
    let pool = &state.pool;
    let row = sqlx::query(
            "SELECT id, display_name, email, password_hash, status, totp_confirmed_at, totp_secret_ciphertext, must_change_password, phone_verified_at, recovery_key_hash, recovery_key_confirmed_at FROM users WHERE lower(email)=lower($1)",
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let id: Uuid = row.try_get("id")?;
    let (role, role_keys, permissions) = load_roles_and_permissions(pool, id).await?;
    let recovery_key_hash: Option<String> = row.try_get("recovery_key_hash")?;
    let recovery_key_confirmed_at: Option<DateTime<Utc>> =
        row.try_get("recovery_key_confirmed_at")?;
    Ok(Some(StoredUser {
        id,
        display_name: row.try_get("display_name")?,
        email: row.try_get("email")?,
        password_hash: row.try_get("password_hash")?,
        role,
        role_keys,
        permissions,
        active: row.try_get::<String, _>("status")? == "active",
        totp_enabled: row
            .try_get::<Option<DateTime<Utc>>, _>("totp_confirmed_at")?
            .is_some(),
        totp_secret_ciphertext: row.try_get("totp_secret_ciphertext")?,
        must_change_password: row.try_get("must_change_password")?,
        must_confirm_recovery_key: recovery_key_hash.is_some()
            && recovery_key_confirmed_at.is_none(),
        phone_verified: row
            .try_get::<Option<DateTime<Utc>>, _>("phone_verified_at")?
            .is_some(),
    }))
}

pub(super) async fn load_roles_and_permissions(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<(String, Vec<String>, Vec<String>), ApiError> {
    let role_rows = sqlx::query(
        "SELECT role.display_name, role.key FROM roles AS role JOIN user_roles ON user_roles.role_id=role.id WHERE user_roles.user_id=$1 ORDER BY role.display_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let role = role_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("display_name").ok())
        .collect::<Vec<_>>()
        .join(" · ");
    let role_keys = role_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("key").ok())
        .collect();
    let permission_rows = sqlx::query(
        r#"SELECT DISTINCT role_permission.permission_key
           FROM role_permissions AS role_permission
           JOIN user_roles ON user_roles.role_id=role_permission.role_id
           WHERE user_roles.user_id=$1 ORDER BY role_permission.permission_key"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let permissions = permission_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("permission_key").ok())
        .collect();
    Ok((role, role_keys, permissions))
}
