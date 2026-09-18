//! Direct media upload and permanent public delivery.
//!
//! The media boundary intentionally performs only bounded multipart parsing,
//! image magic-byte identification, filename cleanup, and SHA-256 hashing.
//! Uploaded objects are public through the API immediately after the catalogue
//! transaction commits; publication controls references, never object access.

mod config;
mod delivery;
mod preview;
mod s3;
mod storage;
mod upload;
mod upload_input;

pub use config::{
    MediaSettings, MediaStorageKind, MediaStorageSettings, DEFAULT_LOCAL_MEDIA_ROOT,
    MAX_ATTACHMENT_BYTES, MAX_MEDIA_UPLOAD_BYTES,
};
pub use delivery::deliver_media_asset;
pub(crate) use preview::{generate_preview, PreviewDerivative};
pub(crate) use storage::probe_storage;
pub(crate) use storage::{delete_object, get_object, put_object};
pub use upload::upload_media_asset;
pub(crate) use upload_input::sniff_media_type;

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
