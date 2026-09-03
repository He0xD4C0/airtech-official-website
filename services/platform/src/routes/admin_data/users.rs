async fn list_users(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<AdminUserRecord>>, ApiError> {
    let mut users = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT user_account.id,user_account.email,user_account.display_name,
                      user_account.locale,user_account.status,user_account.revision,
                      user_account.revision,
                      user_account.totp_confirmed_at IS NOT NULL AS totp_enabled,
                      user_account.invited_at,user_account.last_login_at,
                      user_account.created_at,user_account.updated_at,
                      COALESCE(array_agg(role.key ORDER BY role.key)
                        FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS roles
               FROM users user_account
               LEFT JOIN user_roles assignment ON assignment.user_id=user_account.id
               LEFT JOIN roles role ON role.id=assignment.role_id
               GROUP BY user_account.id ORDER BY user_account.created_at DESC"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(decode_admin_user)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let now = Utc::now();
        state
            .data
            .read()
            .await
            .admin_users
            .values()
            .map(|user| AdminUserRecord {
                id: user.id,
                email: user.email.clone(),
                display_name: user.display_name.clone(),
                locale: "zh-CN".into(),
                status: if user.active { "active" } else { "disabled" }.into(),
                revision: 1,
                roles: vec![user.role.clone()],
                totp_enabled: user.totp_enabled,
                invited_at: None,
                last_login_at: None,
                created_at: now,
                updated_at: now,
            })
            .collect()
    };
    users.sort_by_key(|user| std::cmp::Reverse(user.created_at));
    Ok(Json(paginate_by_id("admin.users", users, query, |user| {
        user.id
    })?))
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let user = load_admin_user(&state, id).await?;
    Ok(entity_response(StatusCode::OK, &user, user.revision))
}

async fn update_user(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(update): Json<UpdateAdminUser>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    if id == principal.user_id
        && update
            .status
            .as_deref()
            .is_some_and(|status| status != "active")
    {
        return Err(ApiError::conflict(
            "The current administrator cannot make their own account non-active.",
        ));
    }
    if update
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "invited" | "active" | "disabled"))
    {
        return Err(ApiError::bad_request("status is invalid."));
    }
    let idempotency_request = json!({"userId": id, "update": &update});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.user.update",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let user: AdminUserRecord = replay.decode()?;
            return Ok(entity_response(status, &user, user.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_admin_user(&state, id).await?;
    if before.revision != expected {
        return Err(ApiError::conflict(
            "The administrator changed; reload before saving.",
        ));
    }
    let after = if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *transaction)
            .await?;
        // Serialize every identity update that can change the active Super
        // Admin set. Locking only the current user rows permits two concurrent
        // removals to both observe another administrator.
        sqlx::query(
            "SELECT pg_advisory_xact_lock(hashtextextended('airtek.identity.super-admin',0))",
        )
        .execute(&mut *transaction)
        .await?;
        let target = sqlx::query(
            r#"SELECT account.status,
                      EXISTS(
                        SELECT 1 FROM user_roles assignment
                        JOIN roles role ON role.id=assignment.role_id
                        WHERE assignment.user_id=account.id AND role.key='super-admin'
                      ) AS is_super_admin
               FROM users account WHERE account.id=$1 FOR UPDATE"#,
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| ApiError::not_found("Administrator was not found."))?;
        let current_status: String = target.try_get("status")?;
        let is_super_admin: bool = target.try_get("is_super_admin")?;
        let target_status_is_active =
            update.status.as_deref().unwrap_or(&current_status) == "active";
        let target_keeps_super_admin = update
            .role_keys
            .as_ref()
            .map(|roles| roles.iter().any(|role| role == "super-admin"))
            .unwrap_or(is_super_admin);
        let removes_super_admin = current_status == "active"
            && is_super_admin
            && (!target_status_is_active || !target_keeps_super_admin);
        if removes_super_admin {
            let active_super_admin_count = sqlx::query_scalar::<_, i64>(
                r#"SELECT count(DISTINCT account.id) FROM users account
                   JOIN user_roles assignment ON assignment.user_id=account.id
                   JOIN roles role ON role.id=assignment.role_id
                   WHERE account.status='active' AND role.key='super-admin'"#,
            )
            .fetch_one(&mut *transaction)
            .await?;
            if active_super_admin_count <= 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The last active Super Admin cannot become non-active or lose that role.",
                ));
            }
        }
        let result = sqlx::query(
            r#"UPDATE users SET display_name=COALESCE($2,display_name),
                      locale=COALESCE($3,locale),status=COALESCE($4,status),updated_at=$5,
                      revision=revision+1
               WHERE id=$1 AND revision=$6"#,
        )
        .bind(id)
        .bind(update.display_name.as_deref())
        .bind(update.locale.as_deref())
        .bind(update.status.as_deref())
        .bind(Utc::now())
        .bind(expected)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The administrator changed; reload before saving.",
            ));
        }
        if let Some(role_keys) = &update.role_keys {
            validate_roles(&mut transaction, role_keys).await?;
            sqlx::query("DELETE FROM user_roles WHERE user_id=$1")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                "INSERT INTO user_roles(user_id,role_id) SELECT $1,id FROM roles WHERE key=ANY($2)",
            )
            .bind(id)
            .bind(role_keys)
            .execute(&mut *transaction)
            .await?;
        }
        if let Some(status) = &update.status {
            if status != &before.status {
                sqlx::query(
                    r#"INSERT INTO user_status_history
                       (id,user_id,from_status,to_status,reason,changed_by,request_id,changed_at)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
                )
                .bind(Uuid::new_v4())
                .bind(id)
                .bind(&before.status)
                .bind(status)
                .bind(&update.reason)
                .bind(principal.user_id)
                .bind(request_id(&headers))
                .bind(Utc::now())
                .execute(&mut *transaction)
                .await?;
            }
        }
        let after = load_admin_user_in_transaction(&mut transaction, id).await?;
        let audit = mutation_audit_event(
            &headers,
            "identity.user.update",
            "user",
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
        after
    } else {
        let removes_super_admin = before.status == "active"
            && before.roles.iter().any(|role| role == "super-admin")
            && (update
                .status
                .as_deref()
                .is_some_and(|status| status != "active")
                || update
                    .role_keys
                    .as_ref()
                    .is_some_and(|roles| !roles.iter().any(|role| role == "super-admin")));
        let mut data = state.data.write().await;
        if removes_super_admin
            && data
                .admin_users
                .values()
                .filter(|user| user.active && user.role == "super-admin")
                .count()
                <= 1
        {
            return Err(ApiError::conflict(
                "The last active Super Admin cannot become non-active or lose that role.",
            ));
        }
        if let Some(user) = data.admin_users.get_mut(&id) {
            if let Some(display_name) = &update.display_name {
                user.display_name = display_name.clone();
            }
            if let Some(status) = &update.status {
                user.active = status == "active";
            }
            if let Some(roles) = &update.role_keys {
                user.role = roles.first().cloned().unwrap_or_else(|| "auditor".into());
            }
        }
        drop(data);
        let after = load_admin_user(&state, id).await?;
        audit_mutation(
            &state,
            &headers,
            "identity.user.update",
            "user",
            Some(id),
            Some(json!(before)),
            Some(json!(after)),
            Some(update.reason),
        )
        .await?;
        idempotency.complete(&state, &after, StatusCode::OK).await?;
        after
    };
    Ok(entity_response(StatusCode::OK, &after, after.revision))
}

async fn revoke_user_sessions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        let sessions_revoked = sqlx::query(
            "UPDATE sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let audit = mutation_audit_event(
            &headers,
            "identity.sessions.revoke",
            "user",
            Some(id),
            None,
            Some(json!({"sessionsRevoked": sessions_revoked})),
            Some("Revoke all sessions for administrator".into()),
        );
        insert_audit_event_in_transaction(&mut transaction, &audit).await?;
        transaction.commit().await?;
    } else {
        for session in state.data.write().await.admin_sessions.values_mut() {
            if session.user_id == id {
                session.revoked = true;
            }
        }
        audit_mutation(
            &state,
            &headers,
            "identity.sessions.revoke",
            "user",
            Some(id),
            None,
            Some(json!({"sessionsRevoked": true})),
            Some("Revoke all sessions for administrator".into()),
        )
        .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
