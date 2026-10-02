use super::*;

pub async fn load_admin_user(state: &AppState, id: Uuid) -> Result<AdminUserRecord, ApiError> {
    let row = sqlx::query(
        r#"SELECT user_account.id,user_account.email,user_account.display_name,
                  user_account.locale,user_account.status,user_account.revision,
                  user_account.manager_user_id,
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
    .fetch_optional(&state.pool)
    .await?;
    row.map(decode_admin_user)
        .transpose()?
        .ok_or_else(|| ApiError::not_found("Administrator was not found."))
}

pub async fn load_admin_user_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<AdminUserRecord, ApiError> {
    let row = sqlx::query(
        r#"SELECT user_account.id,user_account.email,user_account.display_name,
                  user_account.locale,user_account.status,user_account.revision,
                  user_account.manager_user_id,
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

pub async fn load_admin_role(state: &AppState, id: Uuid) -> Result<AdminRoleRecord, ApiError> {
    let row = role_row(&state.pool, id).await?;
    row.map(decode_admin_role)
        .transpose()?
        .ok_or_else(|| ApiError::not_found("Role was not found."))
}

async fn role_row(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
    sqlx::query(
        r#"SELECT role.id,role.key,role.display_name,role.system_role,role.is_preset,role.revision,
                  COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                    FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
           FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
           WHERE role.id=$1 GROUP BY role.id"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn load_admin_role_in_transaction(
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
    decode_admin_role(row)
}

pub fn decode_admin_user(row: sqlx::postgres::PgRow) -> Result<AdminUserRecord, ApiError> {
    Ok(AdminUserRecord {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        locale: row.try_get("locale")?,
        status: row.try_get("status")?,
        revision: row.try_get("revision")?,
        manager_user_id: row.try_get("manager_user_id")?,
        roles: row.try_get("roles")?,
        totp_enabled: row.try_get("totp_enabled")?,
        invited_at: row.try_get("invited_at")?,
        last_login_at: row.try_get("last_login_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub fn decode_admin_role(row: sqlx::postgres::PgRow) -> Result<AdminRoleRecord, ApiError> {
    Ok(AdminRoleRecord {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        display_name: row.try_get("display_name")?,
        system_role: row.try_get("system_role")?,
        is_preset: row.try_get("is_preset")?,
        revision: row.try_get("revision")?,
        permissions: row.try_get("permissions")?,
    })
}

pub fn decode_invitation(row: sqlx::postgres::PgRow) -> Result<UserInvitation, ApiError> {
    let accepted: Option<DateTime<Utc>> = row.try_get("accepted_at")?;
    let revoked: Option<DateTime<Utc>> = row.try_get("revoked_at")?;
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

pub async fn validate_roles(
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

pub struct LockedUser {
    pub status: String,
    pub is_super_admin: bool,
}

pub async fn prepare_user_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<LockedUser, ApiError> {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut **transaction)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('airtek.identity.super-admin',0))")
        .execute(&mut **transaction)
        .await?;
    let row = sqlx::query(
        r#"SELECT account.status,
                  EXISTS(SELECT 1 FROM user_roles assignment
                    JOIN roles role ON role.id=assignment.role_id
                    WHERE assignment.user_id=account.id AND role.key='super-admin') AS is_super_admin
           FROM users account WHERE account.id=$1 FOR UPDATE"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Administrator was not found."))?;
    Ok(LockedUser {
        status: row.try_get("status")?,
        is_super_admin: row.try_get("is_super_admin")?,
    })
}

pub async fn active_super_admin_count(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        r#"SELECT count(DISTINCT account.id) FROM users account
           JOIN user_roles assignment ON assignment.user_id=account.id
           JOIN roles role ON role.id=assignment.role_id
           WHERE account.status='active' AND role.key='super-admin'"#,
    )
    .fetch_one(&mut **transaction)
    .await?)
}

pub async fn update_user_record(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    update: &UpdateAdminUser,
    expected: i64,
) -> Result<u64, ApiError> {
    let result = sqlx::query(
        r#"UPDATE users SET display_name=COALESCE($2,display_name),
                  locale=COALESCE($3,locale),status=COALESCE($4,status),
                  manager_user_id=CASE WHEN $5 THEN $6 ELSE manager_user_id END,
                  updated_at=$7,revision=revision+1
           WHERE id=$1 AND revision=$8"#,
    )
    .bind(id)
    .bind(update.display_name.as_deref())
    .bind(update.locale.as_deref())
    .bind(update.status.as_deref())
    .bind(update.manager_user_id.is_some())
    .bind(update.manager_user_id.flatten())
    .bind(Utc::now())
    .bind(expected)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        if error
            .as_database_error()
            .is_some_and(|value| value.code().as_deref() == Some("23514"))
        {
            ApiError::conflict("The manager relationship would create a cycle.")
        } else {
            error.into()
        }
    })?;
    Ok(result.rows_affected())
}

pub async fn replace_user_roles(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    role_keys: &[String],
) -> Result<(), ApiError> {
    validate_roles(transaction, role_keys).await?;
    sqlx::query("DELETE FROM user_roles WHERE user_id=$1")
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query(
        "INSERT INTO user_roles(user_id,role_id) SELECT $1,id FROM roles WHERE key=ANY($2)",
    )
    .bind(id)
    .bind(role_keys)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub struct UserStatusHistory<'a> {
    pub user_id: Uuid,
    pub from_status: &'a str,
    pub to_status: &'a str,
    pub reason: &'a str,
    pub changed_by: Uuid,
    pub request_id: Uuid,
}

pub async fn insert_user_status_history(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    history: UserStatusHistory<'_>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO user_status_history
           (id,user_id,from_status,to_status,reason,changed_by,request_id,changed_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
    )
    .bind(Uuid::new_v4())
    .bind(history.user_id)
    .bind(history.from_status)
    .bind(history.to_status)
    .bind(history.reason)
    .bind(history.changed_by)
    .bind(history.request_id)
    .bind(Utc::now())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn revoke_user_sessions(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<u64, ApiError> {
    Ok(sqlx::query(
        "UPDATE sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1 AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(&mut **transaction)
    .await?
    .rows_affected())
}

pub async fn validate_permissions(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    permissions: &[String],
) -> Result<(), ApiError> {
    if permissions.len() > 128
        || permissions.iter().collect::<HashSet<_>>().len() != permissions.len()
    {
        return Err(ApiError::bad_request(
            "permissions must contain no more than 128 unique keys.",
        ));
    }
    let known = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM permissions WHERE key=ANY($1)")
        .bind(permissions)
        .fetch_one(&mut **transaction)
        .await?;
    if known != permissions.len() as i64 {
        return Err(ApiError::bad_request(
            "permissions contains an unknown permission key.",
        ));
    }
    Ok(())
}

pub async fn update_role_record(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    update: &UpdateAdminRole,
    expected: i64,
) -> Result<u64, ApiError> {
    Ok(sqlx::query(
        r#"UPDATE roles SET display_name=COALESCE($2,display_name),revision=revision+1
           WHERE id=$1 AND revision=$3 AND key<>'super-admin'"#,
    )
    .bind(id)
    .bind(update.display_name.as_deref().map(str::trim))
    .bind(expected)
    .execute(&mut **transaction)
    .await?
    .rows_affected())
}

pub async fn replace_role_permissions(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    permissions: &[String],
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM role_permissions WHERE role_id=$1")
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    sqlx::query(
        "INSERT INTO role_permissions(role_id,permission_key) SELECT $1,key FROM permissions WHERE key=ANY($2)",
    )
    .bind(id)
    .bind(permissions)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
