use super::*;

pub(super) async fn list_invitations(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<UserInvitation>>, ApiError> {
    Ok(Json(
        crate::services::identity::list_invitations(&state, query).await?,
    ))
}

pub(super) async fn invite_user(
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
    let pool = &state.pool;
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
    crate::services::identity::lock_invitation_email(&mut transaction, &normalized_email).await?;
    let expired_invitation_id =
        crate::services::identity::expire_pending_invitation(&mut transaction, &normalized_email)
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
        crate::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    }
    let (account_exists, pending_exists) =
        crate::services::identity::invitation_conflicts(&mut transaction, &normalized_email)
            .await?;
    if account_exists {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "An administrator account already exists for this email.",
        ));
    }
    if pending_exists {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "A current administrator invitation already exists for this email.",
        ));
    }
    validate_roles(&mut transaction, &input.role_keys).await?;
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
    crate::services::identity::insert_invitation(
        &mut transaction,
        &invitation,
        Sha256::digest(
            invitation
                .invitation_token
                .as_deref()
                .expect("new invitation token exists")
                .as_bytes(),
        )
        .to_vec(),
        principal.user_id,
    )
    .await?;
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
    crate::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
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

pub(super) async fn revoke_invitation(
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
    let pool = &state.pool;
    let mut transaction = pool.begin().await?;
    if crate::services::identity::revoke_invitation(
        &mut transaction,
        id,
        principal.user_id,
        &request.reason,
    )
    .await?
        != 1
    {
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
    crate::services::audit_log::insert_in_transaction(&mut transaction, &audit).await?;
    let replay_receipt = json!({"invitationId": id, "revoked": true});
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &replay_receipt, StatusCode::NO_CONTENT)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
