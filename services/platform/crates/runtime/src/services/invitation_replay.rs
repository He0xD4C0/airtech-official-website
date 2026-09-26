use crate::{config::InvitationReplayEncryptionKey, error::ApiError};
use airtek_domain::models::UserInvitation;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

const INVITATION_REPLAY_ENVELOPE_VERSION: u8 = 1;
const INVITATION_REPLAY_NONCE_BYTES: usize = 12;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EncryptedInvitationReplay {
    pub version: u8,
    pub nonce: String,
    pub ciphertext: String,
}

pub fn seal_invitation_replay(
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

pub fn open_invitation_replay(
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
