use super::*;

pub(super) fn invitation_replay_aad(
    headers: &HeaderMap,
    input: &InviteAdminUser,
) -> Result<String, ApiError> {
    let idempotency_key = parse_idempotency_key(headers)?;
    let key_hash = format!("{:x}", Sha256::digest(idempotency_key.as_bytes()));
    let input = serde_json::to_value(input)
        .map_err(|_| ApiError::internal("Invitation replay binding failed."))?;
    Ok(format!(
        "airtek.invitation-replay.v1|{key_hash}|{}",
        json_hash(&input)
    ))
}

pub(super) fn seal_invitation_replay(
    invitation: &UserInvitation,
    encryption_key: &InvitationReplayEncryptionKey,
    aad: &[u8],
) -> Result<EncryptedInvitationReplay, ApiError> {
    let mut nonce = [0_u8; INVITATION_REPLAY_NONCE_BYTES];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| ApiError::internal("Secure invitation replay nonce generation failed."))?;
    let key = LessSafeKey::new(
        UnboundKey::new(&aead::AES_256_GCM, encryption_key.as_bytes())
            .map_err(|_| ApiError::internal("Invitation replay key initialization failed."))?,
    );
    let mut plaintext = serde_json::to_vec(invitation)
        .map_err(|_| ApiError::internal("Invitation replay serialization failed."))?;
    if key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad),
            &mut plaintext,
        )
        .is_err()
    {
        plaintext.zeroize();
        return Err(ApiError::internal("Invitation replay encryption failed."));
    }
    let envelope = EncryptedInvitationReplay {
        version: INVITATION_REPLAY_ENVELOPE_VERSION,
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        ciphertext: URL_SAFE_NO_PAD.encode(&plaintext),
    };
    plaintext.zeroize();
    Ok(envelope)
}

pub(super) fn open_invitation_replay(
    envelope: &EncryptedInvitationReplay,
    encryption_key: &InvitationReplayEncryptionKey,
    aad: &[u8],
) -> Result<UserInvitation, ApiError> {
    if envelope.version != INVITATION_REPLAY_ENVELOPE_VERSION {
        return Err(ApiError::service_unavailable(
            "Stored invitation replay uses an unsupported envelope version.",
        ));
    }
    let nonce: [u8; INVITATION_REPLAY_NONCE_BYTES] = URL_SAFE_NO_PAD
        .decode(&envelope.nonce)
        .ok()
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| {
            ApiError::service_unavailable("Stored invitation replay nonce is invalid.")
        })?;
    let mut ciphertext = URL_SAFE_NO_PAD.decode(&envelope.ciphertext).map_err(|_| {
        ApiError::service_unavailable("Stored invitation replay ciphertext is invalid.")
    })?;
    if ciphertext.len() < aead::AES_256_GCM.tag_len() {
        ciphertext.zeroize();
        return Err(ApiError::service_unavailable(
            "Stored invitation replay ciphertext is invalid.",
        ));
    }
    let key = LessSafeKey::new(
        UnboundKey::new(&aead::AES_256_GCM, encryption_key.as_bytes()).map_err(|_| {
            ApiError::service_unavailable("Invitation replay key initialization failed.")
        })?,
    );
    let opened_length = match key.open_in_place(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad),
        &mut ciphertext,
    ) {
        Ok(opened) => opened.len(),
        Err(_) => {
            ciphertext.zeroize();
            return Err(ApiError::service_unavailable(
                "Stored invitation replay could not be authenticated.",
            ));
        }
    };
    let invitation = serde_json::from_slice::<UserInvitation>(&ciphertext[..opened_length])
        .map_err(|_| ApiError::service_unavailable("Stored invitation replay payload is invalid."));
    ciphertext.zeroize();
    let invitation = invitation?;
    if invitation
        .invitation_token
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Err(ApiError::service_unavailable(
            "Stored invitation replay token is missing.",
        ));
    }
    Ok(invitation)
}

pub(super) fn validate_invitation(input: &InviteAdminUser) -> Result<(), ApiError> {
    if !input.email.contains('@') || input.email.len() > 254 {
        return Err(ApiError::bad_request("email is invalid."));
    }
    if input.display_name.trim().is_empty() || input.display_name.len() > 200 {
        return Err(ApiError::bad_request(
            "displayName must contain 1 to 200 characters.",
        ));
    }
    if input.role_keys.is_empty() || input.role_keys.len() > 20 {
        return Err(ApiError::bad_request(
            "roleKeys must contain 1 to 20 roles.",
        ));
    }
    Ok(())
}
