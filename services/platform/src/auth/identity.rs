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

fn validate_setup(request: &SetupRequest) -> Result<(), ApiError> {
    if request.display_name.trim().is_empty() || request.display_name.trim().len() > 120 {
        return Err(ApiError::bad_request(
            "displayName must contain 1 to 120 characters.",
        ));
    }
    if !valid_email(request.email.trim()) {
        return Err(ApiError::bad_request("A valid work email is required."));
    }
    validate_strong_password(&request.password)?;
    Ok(())
}

fn validate_strong_password(password: &str) -> Result<(), ApiError> {
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

fn verify_bootstrap_token(state: &AppState, supplied: &str) -> Result<(), ApiError> {
    let expected = state
        .config
        .admin_bootstrap_token
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Initial setup is not enabled."))?;
    if supplied.len() != expected.len()
        || !bool::from(supplied.as_bytes().ct_eq(expected.as_bytes()))
    {
        return Err(ApiError::unauthorized("The bootstrap token is invalid."));
    }
    Ok(())
}

async fn create_initial_super_admin(
    state: &AppState,
    display_name: &str,
    email: &str,
    password_hash: String,
) -> Result<StoredUser, ApiError> {
    let id = Uuid::new_v4();
    let normalized_email = email.to_ascii_lowercase();
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(672183921)")
            .execute(&mut *transaction)
            .await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
            .fetch_one(&mut *transaction)
            .await?;
        if exists {
            return Err(ApiError::conflict(
                "Initial setup is disabled after the first user is created.",
            ));
        }
        let role_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO roles (id, key, display_name, system_role)
               VALUES ($1, 'super-admin', 'Super Admin', true)
               ON CONFLICT (key) DO UPDATE SET display_name=EXCLUDED.display_name
               RETURNING id"#,
        )
        .bind(Uuid::new_v4())
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_key) SELECT $1, key FROM permissions ON CONFLICT DO NOTHING",
        )
        .bind(role_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO users
               (id, email, password_hash, display_name, status, created_at, updated_at)
               VALUES ($1,$2,$3,$4,'active',now(),now())"#,
        )
        .bind(id)
        .bind(&normalized_email)
        .bind(&password_hash)
        .bind(display_name)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1,$2)")
            .bind(id)
            .bind(role_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
    } else {
        let mut data = state.data.write().await;
        if !data.admin_users.is_empty() {
            return Err(ApiError::conflict(
                "Initial setup is disabled after the first user is created.",
            ));
        }
        let user = StoredUser {
            id,
            display_name: display_name.into(),
            email: normalized_email.clone(),
            password_hash: password_hash.clone(),
            role: "Super Admin".into(),
            permissions: super_admin_permissions(),
            active: true,
            totp_enabled: false,
            totp_secret_ciphertext: None,
        };
        data.admin_users.insert(id, user);
    }
    Ok(StoredUser {
        id,
        display_name: display_name.into(),
        email: normalized_email,
        password_hash,
        role: "Super Admin".into(),
        permissions: super_admin_permissions(),
        active: true,
        totp_enabled: false,
        totp_secret_ciphertext: None,
    })
}

async fn find_user_by_email(state: &AppState, email: &str) -> Result<Option<StoredUser>, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "SELECT id, display_name, email, password_hash, status, totp_confirmed_at, totp_secret_ciphertext FROM users WHERE lower(email)=lower($1)",
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let id: Uuid = row.try_get("id")?;
        let (role, permissions) = load_roles_and_permissions(pool, id).await?;
        return Ok(Some(StoredUser {
            id,
            display_name: row.try_get("display_name")?,
            email: row.try_get("email")?,
            password_hash: row.try_get("password_hash")?,
            role,
            permissions,
            active: row.try_get::<String, _>("status")? == "active",
            totp_enabled: row
                .try_get::<Option<DateTime<Utc>>, _>("totp_confirmed_at")?
                .is_some(),
            totp_secret_ciphertext: row.try_get("totp_secret_ciphertext")?,
        }));
    }
    Ok(state
        .data
        .read()
        .await
        .admin_users
        .values()
        .find(|user| user.email.eq_ignore_ascii_case(email))
        .cloned())
}

async fn load_roles_and_permissions(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<(String, Vec<String>), ApiError> {
    let role_rows = sqlx::query(
        "SELECT role.display_name FROM roles AS role JOIN user_roles ON user_roles.role_id=role.id WHERE user_roles.user_id=$1 ORDER BY role.display_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let role = role_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("display_name").ok())
        .collect::<Vec<_>>()
        .join(" · ");
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
    Ok((role, permissions))
}
