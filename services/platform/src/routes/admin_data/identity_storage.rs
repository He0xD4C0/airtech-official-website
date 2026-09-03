async fn load_admin_user(state: &AppState, id: Uuid) -> Result<AdminUserRecord, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT user_account.id,user_account.email,user_account.display_name,
                      user_account.locale,user_account.status,user_account.revision,
                      user_account.totp_confirmed_at IS NOT NULL AS totp_enabled,
                      user_account.invited_at,user_account.last_login_at,
                      user_account.created_at,user_account.updated_at,
                      COALESCE(array_agg(role.key ORDER BY role.key)
                        FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS roles
               FROM users user_account
               LEFT JOIN user_roles assignment ON assignment.user_id=user_account.id
               LEFT JOIN roles role ON role.id=assignment.role_id
               WHERE user_account.id=$1 GROUP BY user_account.id"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        return row
            .map(decode_admin_user)
            .transpose()?
            .ok_or_else(|| ApiError::not_found("Administrator was not found."));
    }
    let user = state
        .data
        .read()
        .await
        .admin_users
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Administrator was not found."))?;
    let now = Utc::now();
    Ok(AdminUserRecord {
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        locale: "zh-CN".into(),
        status: if user.active { "active" } else { "disabled" }.into(),
        revision: 1,
        roles: vec![user.role],
        totp_enabled: user.totp_enabled,
        invited_at: None,
        last_login_at: None,
        created_at: now,
        updated_at: now,
    })
}

async fn load_admin_user_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<AdminUserRecord, ApiError> {
    let row = sqlx::query(
        r#"SELECT user_account.id,user_account.email,user_account.display_name,
                  user_account.locale,user_account.status,user_account.revision,
                  user_account.totp_confirmed_at IS NOT NULL AS totp_enabled,
                  user_account.invited_at,user_account.last_login_at,
                  user_account.created_at,user_account.updated_at,
                  COALESCE(array_agg(role.key ORDER BY role.key)
                    FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS roles
           FROM users user_account
           LEFT JOIN user_roles assignment ON assignment.user_id=user_account.id
           LEFT JOIN roles role ON role.id=assignment.role_id
           WHERE user_account.id=$1 GROUP BY user_account.id"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Administrator was not found."))?;
    decode_admin_user(row)
}

async fn load_admin_role(state: &AppState, id: Uuid) -> Result<AdminRoleRecord, ApiError> {
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Role details require PostgreSQL persistence.",
        ));
    };
    let row = sqlx::query(
        r#"SELECT role.id,role.key,role.display_name,role.system_role,role.revision,
                  COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                    FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
           FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
           WHERE role.id=$1 GROUP BY role.id"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Role was not found."))?;
    Ok(AdminRoleRecord {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        display_name: row.try_get("display_name")?,
        system_role: row.try_get("system_role")?,
        revision: row.try_get("revision")?,
        permissions: row.try_get("permissions")?,
    })
}

async fn load_admin_role_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<AdminRoleRecord, ApiError> {
    let row = sqlx::query(
        r#"SELECT role.id,role.key,role.display_name,role.system_role,role.revision,
                  COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                    FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
           FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
           WHERE role.id=$1 GROUP BY role.id"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Role was not found."))?;
    Ok(AdminRoleRecord {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        display_name: row.try_get("display_name")?,
        system_role: row.try_get("system_role")?,
        revision: row.try_get("revision")?,
        permissions: row.try_get("permissions")?,
    })
}

fn decode_admin_user(row: sqlx::postgres::PgRow) -> Result<AdminUserRecord, ApiError> {
    Ok(AdminUserRecord {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        locale: row.try_get("locale")?,
        status: row.try_get("status")?,
        revision: row.try_get("revision")?,
        roles: row.try_get("roles")?,
        totp_enabled: row.try_get("totp_enabled")?,
        invited_at: row.try_get("invited_at")?,
        last_login_at: row.try_get("last_login_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn decode_invitation(row: sqlx::postgres::PgRow) -> Result<UserInvitation, ApiError> {
    let accepted: Option<chrono::DateTime<Utc>> = row.try_get("accepted_at")?;
    let revoked: Option<chrono::DateTime<Utc>> = row.try_get("revoked_at")?;
    let expires_at = row.try_get("expires_at")?;
    let status = if accepted.is_some() {
        "accepted"
    } else if revoked.is_some() {
        "revoked"
    } else if expires_at <= Utc::now() {
        "expired"
    } else {
        "pending"
    };
    Ok(UserInvitation {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        locale: row.try_get("locale")?,
        role_keys: row.try_get("role_keys")?,
        status: status.into(),
        invited_at: row.try_get("invited_at")?,
        expires_at,
        invitation_token: None,
    })
}

async fn validate_roles(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    role_keys: &[String],
) -> Result<(), ApiError> {
    if role_keys.is_empty() || role_keys.len() > 20 {
        return Err(ApiError::bad_request(
            "roleKeys must contain 1 to 20 roles.",
        ));
    }
    let unique = role_keys.iter().collect::<HashSet<_>>().len() as i64;
    let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM roles WHERE key=ANY($1)")
        .bind(role_keys)
        .fetch_one(&mut **transaction)
        .await?;
    if count != unique || unique != role_keys.len() as i64 {
        return Err(ApiError::bad_request(
            "roleKeys contains an unknown or duplicate role.",
        ));
    }
    Ok(())
}
