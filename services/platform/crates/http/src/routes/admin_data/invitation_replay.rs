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
