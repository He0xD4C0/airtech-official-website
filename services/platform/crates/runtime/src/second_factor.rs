use std::collections::HashSet;

use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use totp_rs::{Algorithm, TOTP};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::config::TotpEncryptionKey;
use crate::error::ApiError;

const SECRET_BYTES: usize = 20;
const NONCE_BYTES: usize = 12;
const CIPHERTEXT_VERSION: u8 = 1;
const RECOVERY_CODE_COUNT: usize = 10;
const RECOVERY_CODE_CHARACTERS: usize = 16;
const RECOVERY_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub struct EnrollmentSecret {
    raw: Vec<u8>,
}

impl EnrollmentSecret {
    pub fn generate() -> Result<Self, ApiError> {
        let mut raw = vec![0_u8; SECRET_BYTES];
        SystemRandom::new()
            .fill(&mut raw)
            .map_err(|_| ApiError::internal("Secure TOTP secret generation failed."))?;
        Ok(Self { raw })
    }

    pub fn from_bytes(raw: Vec<u8>) -> Result<Self, ApiError> {
        if raw.len() != SECRET_BYTES {
            return Err(ApiError::service_unavailable(
                "The stored TOTP secret is invalid.",
            ));
        }
        Ok(Self { raw })
    }

    pub fn base32(&self) -> String {
        data_encoding::BASE32_NOPAD.encode(&self.raw)
    }

    pub fn seal(&self, key: &TotpEncryptionKey, user_id: Uuid) -> Result<Vec<u8>, ApiError> {
        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| ApiError::internal("Secure nonce generation failed."))?;
        let key = LessSafeKey::new(
            UnboundKey::new(&aead::AES_256_GCM, key.as_bytes())
                .map_err(|_| ApiError::internal("TOTP encryption key initialization failed."))?,
        );
        let mut encrypted = self.raw.clone();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(user_id.as_bytes()),
            &mut encrypted,
        )
        .map_err(|_| ApiError::internal("TOTP secret encryption failed."))?;
        let mut envelope = Vec::with_capacity(1 + NONCE_BYTES + encrypted.len());
        envelope.push(CIPHERTEXT_VERSION);
        envelope.extend_from_slice(&nonce_bytes);
        envelope.extend_from_slice(&encrypted);
        encrypted.zeroize();
        Ok(envelope)
    }

    pub fn open(
        ciphertext: &[u8],
        key: &TotpEncryptionKey,
        user_id: Uuid,
    ) -> Result<Self, ApiError> {
        if ciphertext.len() < 1 + NONCE_BYTES + aead::AES_256_GCM.tag_len()
            || ciphertext[0] != CIPHERTEXT_VERSION
        {
            return Err(ApiError::service_unavailable(
                "The stored TOTP secret envelope is invalid.",
            ));
        }
        let nonce_bytes: [u8; NONCE_BYTES] = ciphertext[1..1 + NONCE_BYTES]
            .try_into()
            .map_err(|_| ApiError::service_unavailable("The stored TOTP nonce is invalid."))?;
        let mut plaintext = ciphertext[1 + NONCE_BYTES..].to_vec();
        let key = LessSafeKey::new(
            UnboundKey::new(&aead::AES_256_GCM, key.as_bytes())
                .map_err(|_| ApiError::internal("TOTP encryption key initialization failed."))?,
        );
        let opened = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(user_id.as_bytes()),
                &mut plaintext,
            )
            .map_err(|_| {
                ApiError::service_unavailable(
                    "The stored TOTP secret could not be authenticated with this deployment key.",
                )
            })?;
        let raw = opened.to_vec();
        plaintext.zeroize();
        Self::from_bytes(raw)
    }

    pub fn provisioning_uri(&self, account: &str) -> Result<String, ApiError> {
        Ok(create_totp(&self.raw, account)?.get_url())
    }

    pub fn verify_current(&self, code: &str, account: &str) -> Result<bool, ApiError> {
        if code.len() != 6 || !code.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(false);
        }
        create_totp(&self.raw, account)?
            .check_current(code)
            .map_err(|_| ApiError::service_unavailable("The system clock is unavailable."))
    }

    #[cfg(test)]
    fn generate_at(&self, timestamp: u64, account: &str) -> Result<String, ApiError> {
        Ok(create_totp(&self.raw, account)?.generate(timestamp))
    }

    #[cfg(test)]
    fn verify_at(&self, code: &str, timestamp: u64, account: &str) -> Result<bool, ApiError> {
        Ok(create_totp(&self.raw, account)?.check(code, timestamp))
    }
}

impl Drop for EnrollmentSecret {
    fn drop(&mut self) {
        self.raw.zeroize();
    }
}

fn create_totp(secret: &[u8], account: &str) -> Result<TOTP, ApiError> {
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_vec(),
        Some("AIRTEKPOWER".into()),
        account.into(),
    )
    .map_err(|_| ApiError::internal("TOTP configuration is invalid."))
}

pub fn generate_recovery_codes() -> Result<Vec<String>, ApiError> {
    let random = SystemRandom::new();
    let mut codes = Vec::with_capacity(RECOVERY_CODE_COUNT);
    let mut canonical_codes = HashSet::with_capacity(RECOVERY_CODE_COUNT);
    while codes.len() < RECOVERY_CODE_COUNT {
        let mut bytes = [0_u8; RECOVERY_CODE_CHARACTERS];
        random
            .fill(&mut bytes)
            .map_err(|_| ApiError::internal("Secure recovery-code generation failed."))?;
        let canonical = bytes
            .iter()
            .map(|byte| RECOVERY_ALPHABET[(byte & 31) as usize] as char)
            .collect::<String>();
        if !canonical_codes.insert(canonical.clone()) {
            continue;
        }
        codes.push(format!(
            "{}-{}-{}-{}",
            &canonical[0..4],
            &canonical[4..8],
            &canonical[8..12],
            &canonical[12..16]
        ));
    }
    Ok(codes)
}

pub fn normalize_recovery_code(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .filter(|character| !matches!(character, '-' | ' '))
        .flat_map(char::to_uppercase)
        .collect::<String>();
    (normalized.len() == RECOVERY_CODE_CHARACTERS
        && normalized
            .bytes()
            .all(|byte| RECOVERY_ALPHABET.contains(&byte)))
    .then_some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    #[test]
    fn secret_envelope_round_trips_and_is_bound_to_user() {
        let key = Config::for_test().totp_encryption_key.unwrap();
        let user_id = Uuid::new_v4();
        let secret = EnrollmentSecret::generate().unwrap();
        let sealed = secret.seal(&key, user_id).unwrap();
        assert_ne!(sealed, secret.raw);
        let opened = EnrollmentSecret::open(&sealed, &key, user_id).unwrap();
        assert_eq!(opened.base32(), secret.base32());
        assert!(EnrollmentSecret::open(&sealed, &key, Uuid::new_v4()).is_err());
    }

    #[test]
    fn totp_accepts_the_adjacent_step_but_not_beyond_skew() {
        let secret = EnrollmentSecret::from_bytes(b"12345678901234567890".to_vec()).unwrap();
        let code = secret.generate_at(59, "admin@example.com").unwrap();
        assert!(secret.verify_at(&code, 59, "admin@example.com").unwrap());
        assert!(secret.verify_at(&code, 89, "admin@example.com").unwrap());
        assert!(!secret.verify_at(&code, 120, "admin@example.com").unwrap());
    }

    #[test]
    fn recovery_codes_are_high_entropy_unique_and_normalized() {
        let codes = generate_recovery_codes().unwrap();
        assert_eq!(codes.len(), RECOVERY_CODE_COUNT);
        assert_eq!(
            codes.iter().collect::<HashSet<_>>().len(),
            RECOVERY_CODE_COUNT
        );
        for code in codes {
            let normalized = normalize_recovery_code(&code).expect("valid generated code");
            assert_eq!(normalized.len(), RECOVERY_CODE_CHARACTERS);
            assert_eq!(
                normalize_recovery_code(&code.to_ascii_lowercase()),
                Some(normalized)
            );
        }
        assert!(normalize_recovery_code("not-a-code").is_none());
    }
}
