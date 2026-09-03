async fn list_invitations(
    State(state): State<AppState>,
) -> Result<Json<CursorPage<UserInvitation>>, ApiError> {
    let Some(pool) = &state.pool else {
        return Ok(Json(CursorPage::all(Vec::new())));
    };
    let rows = sqlx::query(
        r#"SELECT invitation.id,invitation.email,invitation.display_name,
                  invitation.locale,invitation.invited_at,invitation.expires_at,
                  invitation.accepted_at,invitation.revoked_at,
                  COALESCE(array_agg(role.key ORDER BY role.key)
                    FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS role_keys
           FROM user_invitations invitation
           LEFT JOIN user_invitation_roles assignment ON assignment.invitation_id=invitation.id
           LEFT JOIN roles role ON role.id=assignment.role_id
           GROUP BY invitation.id ORDER BY invitation.invited_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(Json(CursorPage::all(
        rows.into_iter()
            .map(decode_invitation)
            .collect::<Result<Vec<_>, _>>()?,
    )))
}

async fn invite_user(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(input): Json<InviteAdminUser>,
) -> Result<Response, ApiError> {
    validate_invitation(&input)?;
    let replay_key = state
        .config
        .invitation_replay_encryption_key
        .as_ref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Invitation replay encryption is not configured for this deployment.",
            )
        })?;
    let replay_aad = invitation_replay_aad(&headers, &input)?;
    let idempotency =
        match begin_idempotency(&state, "admin.identity.invitation.create", &headers, &input)
            .await?
        {
            IdempotencyOutcome::Replay(replay) => {
                let status = replay.status()?;
                let encrypted: EncryptedInvitationReplay = replay.decode()?;
                let invitation =
                    open_invitation_replay(&encrypted, replay_key, replay_aad.as_bytes())?;
                let mut response = (status, Json(invitation)).into_response();
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-store, max-age=0"),
                );
                return Ok(response);
            }
            IdempotencyOutcome::Fresh(context) => context,
        };
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Invitations require PostgreSQL persistence.",
        ));
    };
    let mut token_bytes = [0_u8; 32];
    SystemRandom::new()
        .fill(&mut token_bytes)
        .map_err(|_| ApiError::internal("Secure invitation token generation failed."))?;
    let token = URL_SAFE_NO_PAD.encode(token_bytes);
    let now = Utc::now();
    let id = Uuid::new_v4();
    let expires_at = now + Duration::days(7);
    let normalized_email = input.email.trim().to_lowercase();
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "airtek.identity.invitation.email:{normalized_email}"
        ))
        .execute(&mut *transaction)
        .await?;
    let expired_invitation_id = sqlx::query_scalar::<_, Uuid>(
        r#"UPDATE user_invitations SET status='expired'
           WHERE lower(email)=lower($1) AND status='pending' AND expires_at<=now()
             AND accepted_at IS NULL AND revoked_at IS NULL
           RETURNING id"#,
    )
    .bind(&normalized_email)
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(expired_id) = expired_invitation_id {
        let audit = mutation_audit_event(
            &headers,
            "identity.invitation.expire",
            "userInvitation",
            Some(expired_id),
            Some(json!({"status": "pending", "email": normalized_email})),
            Some(json!({"status": "expired", "email": normalized_email})),
            Some("Expire superseded administrator invitation".into()),
        );
        insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    }
    let account_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($1))")
            .bind(&normalized_email)
            .fetch_one(&mut *transaction)
            .await?;
    if account_exists {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "An administrator account already exists for this email.",
        ));
    }
    let pending_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_invitations WHERE lower(email)=lower($1) AND status='pending')",
    )
    .bind(&normalized_email)
    .fetch_one(&mut *transaction)
    .await?;
    if pending_exists {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "A current administrator invitation already exists for this email.",
        ));
    }
    validate_roles(&mut transaction, &input.role_keys).await?;
    sqlx::query(
        r#"INSERT INTO user_invitations
           (id,email,display_name,locale,token_hash,invited_by,invited_at,expires_at)
           VALUES ($1,$2,$3,'zh-CN',$4,$5,$6,$7)"#,
    )
    .bind(id)
    .bind(&normalized_email)
    .bind(input.display_name.trim())
    .bind(Sha256::digest(token.as_bytes()).to_vec())
    .bind(principal.user_id)
    .bind(now)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO user_invitation_roles(invitation_id,role_id) SELECT $1,id FROM roles WHERE key=ANY($2)",
    )
    .bind(id)
    .bind(&input.role_keys)
    .execute(&mut *transaction)
    .await?;
    let invitation = UserInvitation {
        id,
        email: normalized_email,
        display_name: input.display_name.trim().to_owned(),
        locale: "zh-CN".into(),
        role_keys: input.role_keys,
        status: "pending".into(),
        invited_at: now,
        expires_at,
        invitation_token: Some(token),
    };
    let audit = mutation_audit_event(
        &headers,
        "identity.invitation.create",
        "userInvitation",
        Some(id),
        None,
        Some(json!({
            "id": invitation.id,
            "email": invitation.email,
            "roles": invitation.role_keys,
            "expiresAt": invitation.expires_at,
        })),
        Some("Invite administrator".into()),
    );
    insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    let encrypted_replay = seal_invitation_replay(&invitation, replay_key, replay_aad.as_bytes())?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &encrypted_replay, StatusCode::CREATED)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    let mut response = (StatusCode::CREATED, Json(invitation)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}

async fn revoke_invitation(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<ReasonRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    let idempotency_request = json!({"invitationId": id, "request": &request});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.invitation.revoke",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let _: Value = replay.decode()?;
            return Ok(status.into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Invitations require PostgreSQL persistence.",
        ));
    };
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        r#"UPDATE user_invitations SET status='revoked',revoked_at=now(),revoked_by=$2,revoke_reason=$3
           WHERE id=$1 AND status='pending' AND expires_at>now()
             AND accepted_at IS NULL AND revoked_at IS NULL"#,
    )
    .bind(id)
    .bind(principal.user_id)
    .bind(&request.reason)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "Invitation is missing, accepted, or already revoked.",
        ));
    }
    let audit = mutation_audit_event(
        &headers,
        "identity.invitation.revoke",
        "userInvitation",
        Some(id),
        None,
        Some(json!({"revoked": true})),
        Some(request.reason),
    );
    insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    let replay_receipt = json!({"invitationId": id, "revoked": true});
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &replay_receipt, StatusCode::NO_CONTENT)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
