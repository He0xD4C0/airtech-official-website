use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::ProductStagingEncryptionKey, error::ApiError,
    services::product_import::encrypt_confidential,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuResumeCursor {
    pub table_index: usize,
    pub page_token: Option<String>,
    #[serde(default)]
    pub record_index: usize,
}

pub fn seal_cursor(
    key: &ProductStagingEncryptionKey,
    run_id: Uuid,
    cursor: &FeishuResumeCursor,
) -> Result<String, ApiError> {
    let plaintext = serde_json::to_vec(cursor)
        .map_err(|_| ApiError::internal("Feishu cursor serialization failed."))?;
    let encrypted = encrypt_confidential(key, run_id.as_bytes(), &plaintext)?;
    Ok(URL_SAFE_NO_PAD.encode(encrypted))
}

pub fn open_cursor(
    key: &ProductStagingEncryptionKey,
    run_id: Uuid,
    value: &str,
) -> Result<FeishuResumeCursor, ApiError> {
    let encrypted = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| invalid_cursor())?;
    if encrypted.len() < 28 {
        return Err(invalid_cursor());
    }
    let nonce: [u8; 12] = encrypted[..12].try_into().map_err(|_| invalid_cursor())?;
    let mut ciphertext = encrypted[12..].to_vec();
    let key = LessSafeKey::new(
        UnboundKey::new(&aead::AES_256_GCM, key.as_bytes())
            .map_err(|_| ApiError::internal("Feishu cursor decryption setup failed."))?,
    );
    let plaintext = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(run_id.as_bytes()),
            &mut ciphertext,
        )
        .map_err(|_| invalid_cursor())?;
    serde_json::from_slice(plaintext).map_err(|_| invalid_cursor())
}

fn invalid_cursor() -> ApiError {
    ApiError::service_unavailable("Stored Feishu resume cursor is invalid.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_round_trip_is_authenticated_to_the_run() {
        let config = crate::config::Config::for_test();
        let key = config.product_staging_encryption_key.as_ref().unwrap();
        let run_id = Uuid::new_v4();
        let cursor = FeishuResumeCursor {
            table_index: 2,
            page_token: Some("opaque".into()),
            record_index: 7,
        };
        let sealed = seal_cursor(key, run_id, &cursor).unwrap();
        let opened = open_cursor(key, run_id, &sealed).unwrap();
        assert_eq!(opened.table_index, 2);
        assert_eq!(opened.page_token.as_deref(), Some("opaque"));
        assert!(open_cursor(key, Uuid::new_v4(), &sealed).is_err());
    }
}
