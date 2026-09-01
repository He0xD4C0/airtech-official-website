use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use ring::{hmac, rand::SecureRandom, rand::SystemRandom};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{config::PreviewSigningKey, error::ApiError};

pub const MAX_PREVIEW_TTL_SECONDS: u32 = 10 * 60;
pub const MAX_PREVIEW_TOKEN_LENGTH: usize = 2_048;

const TOKEN_PREFIX: &str = "v1";
const TOKEN_VERSION: u8 = 1;
const NONCE_BYTES: usize = 16;
const SIGNATURE_BYTES: usize = 32;
const MAX_PAYLOAD_BYTES: usize = 1_024;
const MAX_CLOCK_SKEW_SECONDS: i64 = 30;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreviewClaims {
    version: u8,
    issued_at: i64,
    expires_at: i64,
    content_id: Uuid,
    revision: i64,
    nonce: String,
}

#[derive(Clone, Debug)]
pub struct IssuedPreviewToken {
    pub token: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedPreviewToken {
    pub content_id: Uuid,
    pub revision: i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewTokenError {
    Invalid,
    Expired,
}

pub fn issue(
    key: &PreviewSigningKey,
    content_id: Uuid,
    revision: i64,
    ttl_seconds: u32,
) -> Result<IssuedPreviewToken, ApiError> {
    issue_at(key, content_id, revision, ttl_seconds, Utc::now())
}

fn issue_at(
    key: &PreviewSigningKey,
    content_id: Uuid,
    revision: i64,
    ttl_seconds: u32,
    now: DateTime<Utc>,
) -> Result<IssuedPreviewToken, ApiError> {
    if content_id.is_nil() || revision < 1 {
        return Err(ApiError::bad_request(
            "A valid content id and revision are required for preview.",
        ));
    }
    if !(1..=MAX_PREVIEW_TTL_SECONDS).contains(&ttl_seconds) {
        return Err(ApiError::bad_request(format!(
            "expiresInSeconds must contain 1 to {MAX_PREVIEW_TTL_SECONDS}."
        )));
    }
    let expires_at = now + chrono::Duration::seconds(i64::from(ttl_seconds));
    let mut nonce = [0_u8; NONCE_BYTES];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| ApiError::internal("Secure preview nonce generation failed."))?;
    let claims = PreviewClaims {
        version: TOKEN_VERSION,
        issued_at: now.timestamp(),
        expires_at: expires_at.timestamp(),
        content_id,
        revision,
        nonce: URL_SAFE_NO_PAD.encode(nonce),
    };
    let payload = serde_json::to_vec(&claims)
        .map_err(|_| ApiError::internal("Preview claims serialization failed."))?;
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(ApiError::internal("Preview claims exceed the safe limit."));
    }
    let encoded_payload = URL_SAFE_NO_PAD.encode(payload);
    let signing_input = format!("{TOKEN_PREFIX}.{encoded_payload}");
    let signing_key = hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes());
    let signature = hmac::sign(&signing_key, signing_input.as_bytes());
    let token = format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.as_ref())
    );
    if token.len() > MAX_PREVIEW_TOKEN_LENGTH {
        return Err(ApiError::internal("Preview token exceeds the safe limit."));
    }
    Ok(IssuedPreviewToken {
        token,
        issued_at: now,
        expires_at,
    })
}

pub fn verify(
    key: &PreviewSigningKey,
    token: &str,
) -> Result<VerifiedPreviewToken, PreviewTokenError> {
    verify_at(key, token, Utc::now())
}

fn verify_at(
    key: &PreviewSigningKey,
    token: &str,
    now: DateTime<Utc>,
) -> Result<VerifiedPreviewToken, PreviewTokenError> {
    if !(16..=MAX_PREVIEW_TOKEN_LENGTH).contains(&token.len()) {
        return Err(PreviewTokenError::Invalid);
    }
    let mut segments = token.split('.');
    let prefix = segments.next().ok_or(PreviewTokenError::Invalid)?;
    let encoded_payload = segments.next().ok_or(PreviewTokenError::Invalid)?;
    let encoded_signature = segments.next().ok_or(PreviewTokenError::Invalid)?;
    if prefix != TOKEN_PREFIX || segments.next().is_some() {
        return Err(PreviewTokenError::Invalid);
    }
    let payload = URL_SAFE_NO_PAD
        .decode(encoded_payload)
        .map_err(|_| PreviewTokenError::Invalid)?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
        return Err(PreviewTokenError::Invalid);
    }
    let signature = URL_SAFE_NO_PAD
        .decode(encoded_signature)
        .map_err(|_| PreviewTokenError::Invalid)?;
    if signature.len() != SIGNATURE_BYTES {
        return Err(PreviewTokenError::Invalid);
    }
    let signing_input = format!("{prefix}.{encoded_payload}");
    let signing_key = hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes());
    hmac::verify(&signing_key, signing_input.as_bytes(), &signature)
        .map_err(|_| PreviewTokenError::Invalid)?;

    let claims: PreviewClaims =
        serde_json::from_slice(&payload).map_err(|_| PreviewTokenError::Invalid)?;
    let lifetime = claims
        .expires_at
        .checked_sub(claims.issued_at)
        .ok_or(PreviewTokenError::Invalid)?;
    if claims.version != TOKEN_VERSION
        || claims.content_id.is_nil()
        || claims.revision < 1
        || !(1..=i64::from(MAX_PREVIEW_TTL_SECONDS)).contains(&lifetime)
        || claims.issued_at > now.timestamp() + MAX_CLOCK_SKEW_SECONDS
    {
        return Err(PreviewTokenError::Invalid);
    }
    let nonce = URL_SAFE_NO_PAD
        .decode(claims.nonce)
        .map_err(|_| PreviewTokenError::Invalid)?;
    if nonce.len() != NONCE_BYTES {
        return Err(PreviewTokenError::Invalid);
    }
    if now.timestamp() >= claims.expires_at {
        return Err(PreviewTokenError::Expired);
    }
    let expires_at =
        DateTime::from_timestamp(claims.expires_at, 0).ok_or(PreviewTokenError::Invalid)?;
    Ok(VerifiedPreviewToken {
        content_id: claims.content_id,
        revision: claims.revision,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn key() -> PreviewSigningKey {
        Config::for_test().preview_signing_key.unwrap()
    }

    #[test]
    fn signed_preview_round_trips_with_exact_revision_and_unique_nonce() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let content_id = Uuid::new_v4();
        let first = issue_at(&key(), content_id, 7, 600, now).unwrap();
        let second = issue_at(&key(), content_id, 7, 600, now).unwrap();
        assert_ne!(first.token, second.token);
        let verified = verify_at(&key(), &first.token, now).unwrap();
        assert_eq!(verified.content_id, content_id);
        assert_eq!(verified.revision, 7);
        assert_eq!(verified.expires_at, first.expires_at);
        assert_eq!(first.issued_at, now);
    }

    #[test]
    fn tampered_and_overlong_tokens_fail_closed() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let issued = issue_at(&key(), Uuid::new_v4(), 1, 600, now).unwrap();
        let mut tampered = issued.token.clone().into_bytes();
        let index = tampered.len() / 2;
        tampered[index] = if tampered[index] == b'A' { b'B' } else { b'A' };
        let tampered = String::from_utf8(tampered).unwrap();
        assert_eq!(
            verify_at(&key(), &tampered, now),
            Err(PreviewTokenError::Invalid)
        );
        assert_eq!(
            verify_at(&key(), &"x".repeat(MAX_PREVIEW_TOKEN_LENGTH + 1), now),
            Err(PreviewTokenError::Invalid)
        );
    }

    #[test]
    fn expired_token_is_distinct_from_invalid_and_ttl_is_capped() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let issued = issue_at(&key(), Uuid::new_v4(), 3, 10, now).unwrap();
        let after_expiry = now + chrono::Duration::seconds(10);
        assert_eq!(
            verify_at(&key(), &issued.token, after_expiry),
            Err(PreviewTokenError::Expired)
        );
        assert!(issue_at(&key(), Uuid::new_v4(), 1, MAX_PREVIEW_TTL_SECONDS + 1, now).is_err());
    }
}
