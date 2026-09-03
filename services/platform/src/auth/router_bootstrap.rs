pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/invitations/accept", post(accept_invitation))
        .route("/auth/session", get(session))
        .route("/auth/logout", post(logout))
        .route("/auth/totp/enrollment", post(start_totp_enrollment))
        .route("/auth/totp/confirm", post(confirm_totp_enrollment))
        .route(
            "/auth/recovery-codes/regenerate",
            post(regenerate_recovery_codes),
        )
        .route("/auth/sessions", get(list_sessions))
        .route(
            "/auth/sessions/{id}",
            axum::routing::delete(revoke_session_by_id),
        )
}

async fn accept_invitation(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<AcceptInvitationRequest>,
) -> Result<Response, ApiError> {
    validate_strong_password(&request.password)?;
    let token_digest = token_hash(&request.token);
    let account_key = hex_digest(&token_digest);
    let rate_keys = rate_limit_keys(
        "invitation-accept",
        &request_source(&state, &headers, peer),
        &account_key,
    );
    check_auth_rate_limits(&state, &rate_keys).await?;
    if request.token.len() != 43
        || URL_SAFE_NO_PAD
            .decode(request.token.as_bytes())
            .ok()
            .is_none_or(|decoded| decoded.len() != 32)
    {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    }

    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Invitation acceptance requires PostgreSQL persistence.",
        ));
    };
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let password_hash = hash_password(&request.password)?;
    let now = Utc::now();
    let request_id = request_id(&headers);
    let mut transaction = pool.begin().await?;
    let invitation = sqlx::query(
        r#"SELECT id,email,display_name,locale,status,invited_by,invited_at,expires_at
           FROM user_invitations WHERE token_hash=$1 FOR UPDATE"#,
    )
    .bind(&token_digest)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(invitation) = invitation else {
        transaction.rollback().await?;
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    };
    let invitation_id: Uuid = invitation.try_get("id")?;
    let status: String = invitation.try_get("status")?;
    let expires_at: DateTime<Utc> = invitation.try_get("expires_at")?;
    if status != "pending" || expires_at <= now {
        if status == "pending" && expires_at <= now {
            sqlx::query(
                "UPDATE user_invitations SET status='expired' WHERE id=$1 AND status='pending'",
            )
            .bind(invitation_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        } else {
            transaction.rollback().await?;
        }
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    }

    let email: String = invitation.try_get("email")?;
    let display_name: String = invitation.try_get("display_name")?;
    let locale: String = invitation.try_get("locale")?;
    let invited_by: Uuid = invitation.try_get("invited_by")?;
    let invited_at: DateTime<Utc> = invitation.try_get("invited_at")?;
    let email_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($1))")
            .bind(&email)
            .fetch_one(&mut *transaction)
            .await?;
    if email_exists {
        transaction.rollback().await?;
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::conflict(
            "An administrator account already exists for this invitation email.",
        ));
    }

    let role_keys = sqlx::query_scalar::<_, String>(
        r#"SELECT role.key FROM user_invitation_roles assignment
           JOIN roles role ON role.id=assignment.role_id
           WHERE assignment.invitation_id=$1 ORDER BY role.key"#,
    )
    .bind(invitation_id)
    .fetch_all(&mut *transaction)
    .await?;
    if role_keys.is_empty() {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "The invitation has no assignable administrator role.",
        ));
    }

    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO users
           (id,email,password_hash,display_name,locale,status,invited_by,invited_at,created_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,'active',$6,$7,$8,$8)"#,
    )
    .bind(user_id)
    .bind(&email)
    .bind(&password_hash)
    .bind(&display_name)
    .bind(&locale)
    .bind(invited_by)
    .bind(invited_at)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO user_roles(user_id,role_id)
           SELECT $1,role_id FROM user_invitation_roles WHERE invitation_id=$2"#,
    )
    .bind(user_id)
    .bind(invitation_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO user_status_history
           (id,user_id,from_status,to_status,reason,changed_by,request_id,changed_at)
           VALUES ($1,$2,'invited','active','Accepted administrator invitation',$3,$4,$5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(invited_by)
    .bind(request_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    let updated = sqlx::query(
        r#"UPDATE user_invitations
           SET status='accepted',accepted_at=$2,accepted_user_id=$3
           WHERE id=$1 AND status='pending' AND expires_at>$2"#,
    )
    .bind(invitation_id)
    .bind(now)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() != 1 {
        transaction.rollback().await?;
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    }
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,request_id,occurred_at)
           VALUES ($1,'invitation-acceptance','identity.invitation.accept','userInvitation',$2,
                   $3,$4,'Accept administrator invitation',$5,$6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(invitation_id)
    .bind(json!({"status": "pending"}))
    .bind(json!({
        "status": "accepted",
        "acceptedUserId": user_id,
        "roleKeys": role_keys,
    }))
    .bind(request_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;

    let mut response = (
        StatusCode::CREATED,
        Json(InvitationAcceptance {
            user_id,
            email,
            display_name,
            locale,
            role_keys,
            status: "active",
            accepted_at: now,
        }),
    )
        .into_response();
    append_no_store(response.headers_mut());
    Ok(response)
}

fn invalid_invitation() -> ApiError {
    ApiError::unauthorized("The invitation token is invalid, expired, revoked, or already used.")
}

async fn setup(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> Result<Response, ApiError> {
    validate_setup(&request)?;
    let rate_keys = rate_limit_keys(
        "setup",
        &request_source(&state, &headers, peer),
        &request.email,
    );
    check_auth_rate_limits(&state, &rate_keys).await?;
    if has_admin_users(&state).await? {
        return Err(ApiError::conflict(
            "Initial setup is disabled after the first user is created.",
        ));
    }
    if let Err(error) = verify_bootstrap_token(&state, &request.bootstrap_token) {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(error);
    }
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let password_hash = hash_password(&request.password)?;
    let user = create_initial_super_admin(
        &state,
        request.display_name.trim(),
        request.email.trim(),
        password_hash,
    )
    .await?;
    let issue = create_session(&state, user).await?;
    record_auth_audit(
        &state,
        request.email.trim(),
        "auth.initial_setup",
        issue.principal.user_id,
        json!({"role": "Super Admin"}),
        &headers,
    )
    .await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    Ok(session_response(&state, issue, StatusCode::CREATED))
}

async fn has_admin_users(state: &AppState) -> Result<bool, ApiError> {
    if let Some(pool) = &state.pool {
        return Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
            .fetch_one(pool)
            .await?);
    }
    Ok(!state.data.read().await.admin_users.is_empty())
}
