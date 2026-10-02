use super::*;

pub async fn role_permission_map(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    role_keys: &[String],
) -> Result<std::collections::BTreeMap<String, Vec<String>>, ApiError> {
    let rows = sqlx::query(
        r#"SELECT role.key, permission.permission_key
           FROM roles role
           LEFT JOIN role_permissions permission ON permission.role_id=role.id
           WHERE role.key=ANY($1)"#,
    )
    .bind(role_keys)
    .fetch_all(&mut **transaction)
    .await?;
    let mut map: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for row in rows {
        let key: String = row.try_get("key")?;
        let permission: Option<String> = row.try_get("permission_key")?;
        let entry = map.entry(key).or_default();
        if let Some(permission) = permission {
            entry.push(permission);
        }
    }
    Ok(map)
}

pub async fn user_permission_set(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
) -> Result<Vec<String>, ApiError> {
    Ok(sqlx::query_scalar::<_, String>(
        r#"SELECT DISTINCT permission.permission_key
           FROM user_roles assignment
           JOIN role_permissions permission ON permission.role_id=assignment.role_id
           WHERE assignment.user_id=$1"#,
    )
    .bind(user_id)
    .fetch_all(&mut **transaction)
    .await?)
}

/// Reject grants that would let the acting administrator exceed its own
/// authority. Super Admin is the only role allowed to hand out super-admin or
/// `identity.manage`, or to grant a permission it does not itself hold.
pub async fn validate_role_grant(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor_is_super_admin: bool,
    actor_permissions: &[String],
    role_keys: &[String],
) -> Result<(), ApiError> {
    super::validate_roles(transaction, role_keys).await?;
    if actor_is_super_admin {
        return Ok(());
    }
    let held = actor_permissions.iter().collect::<HashSet<_>>();
    let map = role_permission_map(transaction, role_keys).await?;
    for (key, permissions) in &map {
        if key == "super-admin" || permissions.iter().any(|value| value == "identity.manage") {
            return Err(ApiError::forbidden(
                "Only a Super Admin can assign administrator-management roles.",
            ));
        }
        if permissions.iter().any(|value| !held.contains(value)) {
            return Err(ApiError::forbidden(
                "A role may only grant permissions the acting administrator already holds.",
            ));
        }
    }
    Ok(())
}

pub async fn validate_user_manage_scope(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor_is_super_admin: bool,
    actor_permissions: &[String],
    target_user_id: Uuid,
) -> Result<(), ApiError> {
    if actor_is_super_admin {
        return Ok(());
    }
    let held = actor_permissions.iter().collect::<HashSet<_>>();
    let target = user_permission_set(transaction, target_user_id).await?;
    if target.iter().any(|value| !held.contains(value)) {
        return Err(ApiError::forbidden(
            "The target administrator holds permissions the acting administrator does not.",
        ));
    }
    Ok(())
}

pub async fn role_assignment_count(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    role_id: Uuid,
) -> Result<i64, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM user_roles WHERE role_id=$1")
            .bind(role_id)
            .fetch_one(&mut **transaction)
            .await?,
    )
}

pub async fn role_key_exists(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &str,
) -> Result<bool, ApiError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM roles WHERE key=$1)")
            .bind(key)
            .fetch_one(&mut **transaction)
            .await?,
    )
}

pub async fn insert_role(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    key: &str,
    display_name: &str,
    permissions: &[String],
    created_by: Uuid,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO roles (id, key, display_name, system_role, is_preset, created_by)
           VALUES ($1,$2,$3,false,false,$4)"#,
    )
    .bind(id)
    .bind(key)
    .bind(display_name)
    .bind(created_by)
    .execute(&mut **transaction)
    .await?;
    super::replace_role_permissions(transaction, id, permissions).await?;
    Ok(())
}

pub async fn delete_role_record(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    expected: i64,
) -> Result<u64, ApiError> {
    Ok(sqlx::query(
        "DELETE FROM roles WHERE id=$1 AND revision=$2 AND is_preset=false AND key<>'super-admin'",
    )
    .bind(id)
    .bind(expected)
    .execute(&mut **transaction)
    .await?
    .rows_affected())
}
