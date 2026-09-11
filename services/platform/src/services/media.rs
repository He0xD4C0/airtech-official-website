//! Media upload pipeline.
//!
//! The catalogue read paths live in [`crate::services::media_assets`]. This
//! module owns the write side: byte validation, object storage, the human
//! review gate, and the immutable public delivery path.
//!
//! Everything here fails closed. When object storage is not configured the
//! upload endpoints refuse work instead of writing to a local fallback the
//! deployment never intended to serve.

mod config;
mod delivery;
mod s3;
mod storage;
mod upload;

pub use config::{
    MediaSettings, MediaStorageKind, MediaStorageSettings, MAX_MEDIA_UPLOAD_BYTES,
};
pub use delivery::deliver_media_asset;
pub use upload::{
    parse_upload_body, review_media_asset, sniff_media_type, upload_media_asset, ParsedUpload,
};

/// Rejects object keys that could escape the configured storage prefix.
fn validate_storage_key(key: &str) -> Result<(), crate::error::ApiError> {
    let valid = !key.is_empty()
        && key.len() <= 512
        && !key.starts_with('/')
        && !key.ends_with('/')
        && key
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./".contains(character));
    if valid {
        Ok(())
    } else {
        Err(crate::error::ApiError::internal(
            "Media storage key is invalid.",
        ))
    }
}
