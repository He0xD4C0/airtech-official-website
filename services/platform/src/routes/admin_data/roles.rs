async fn list_roles(
    State(state): State<AppState>,
    Query(query): Query<IdentityListQuery>,
) -> Result<Json<crate::models::AdminRolePage>, ApiError> {
    let search = identity_query_text(query.q)?.map(|value| value.to_lowercase());
    let roles = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT role.id,role.key,role.display_name,role.system_role,role.revision,
                      COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                        FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
               FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
               GROUP BY role.id ORDER BY role.display_name"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AdminRoleRecord {
                    id: row.try_get("id")?,
                    key: row.try_get("key")?,
                    display_name: row.try_get("display_name")?,
                    system_role: row.try_get("system_role")?,
                    revision: row.try_get("revision")?,
                    permissions: row.try_get("permissions")?,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        let data = state.data.read().await;
        let mut seen = HashSet::new();
        data.admin_users
            .values()
            .filter(|user| seen.insert(user.role.clone()))
            .map(|user| AdminRoleRecord {
                id: Uuid::new_v4(),
                key: user.role.clone(),
                display_name: user.role.clone(),
                system_role: true,
                revision: 1,
                permissions: user.permissions.clone(),
            })
            .collect()
    };
    let mut roles = roles;
    roles.retain(|role| search.as_ref().is_none_or(|needle| {
        format!("{} {} {}", role.key, role.display_name, role.permissions.join(" "))
            .to_lowercase()
            .contains(needle)
    }));
    let total = roles.len();
    let scope = format!("admin.roles|{search:?}");
    let page = crate::pagination::paginate_by_id_scoped(
        &scope,
        roles,
        CursorQuery { cursor: query.cursor, limit: query.limit },
        |role| role.id,
    )?;
    Ok(Json(crate::models::AdminRolePage {
        items: page.items,
        next_cursor: page.next_cursor,
        total,
    }))
}

async fn get_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let role = load_admin_role(&state, id).await?;
    Ok(entity_response(StatusCode::OK, &role, role.revision))
}

async fn update_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(update): Json<UpdateAdminRole>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    if let Some(display_name) = update.display_name.as_deref() {
        if display_name.trim().is_empty() || display_name.len() > 120 {
            return Err(ApiError::bad_request(
                "displayName must contain 1 to 120 characters.",
            ));
        }
    }
    let idempotency_request = json!({"roleId": id, "update": &update});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.role.update",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let role: AdminRoleRecord = replay.decode()?;
            return Ok(entity_response(status, &role, role.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_admin_role(&state, id).await?;
    if before.revision != expected {
        return Err(ApiError::conflict(
            "The role changed; reload before saving.",
        ));
    }
    if before.key == "super-admin" {
        return Err(ApiError::forbidden(
            "The Super Admin role template is protected from permission edits.",
        ));
    }
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Role editing requires PostgreSQL persistence.",
        ));
    };
    let mut transaction = pool.begin().await?;
    if let Some(permissions) = &update.permissions {
        if permissions.len() > 128
            || permissions.iter().collect::<HashSet<_>>().len() != permissions.len()
        {
            return Err(ApiError::bad_request(
                "permissions must contain no more than 128 unique keys.",
            ));
        }
        let known =
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM permissions WHERE key=ANY($1)")
                .bind(permissions)
                .fetch_one(&mut *transaction)
                .await?;
        if known != permissions.len() as i64 {
            return Err(ApiError::bad_request(
                "permissions contains an unknown permission key.",
            ));
        }
    }
    let result = sqlx::query(
        r#"UPDATE roles SET display_name=COALESCE($2,display_name),revision=revision+1
           WHERE id=$1 AND revision=$3 AND key<>'super-admin'"#,
    )
    .bind(id)
    .bind(update.display_name.as_deref().map(str::trim))
    .bind(expected)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "The role changed; reload before saving.",
        ));
    }
    if let Some(permissions) = &update.permissions {
        sqlx::query("DELETE FROM role_permissions WHERE role_id=$1")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO role_permissions(role_id,permission_key) SELECT $1,key FROM permissions WHERE key=ANY($2)",
        )
        .bind(id)
        .bind(permissions)
        .execute(&mut *transaction)
        .await?;
    }
    let after = load_admin_role_in_transaction(&mut transaction, id).await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.role.update",
        "role",
        Some(id),
        Some(json!(before)),
        Some(json!(after)),
        Some(update.reason),
    );
    insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(entity_response(StatusCode::OK, &after, after.revision))
}
