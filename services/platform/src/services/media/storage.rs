use std::{fs, io::Write, path::PathBuf};

use uuid::Uuid;

use crate::error::ApiError;

use super::config::{MediaStorageKind, MediaStorageSettings};
use super::s3;
use super::{validate_storage_key, MAX_ATTACHMENT_BYTES};

pub(crate) async fn probe_storage(
    settings: &MediaStorageSettings,
    key: &str,
    public_url: &str,
) -> Result<(), ApiError> {
    let payload = b"airtek-object-storage-probe".to_vec();
    put_object(settings, key, "text/plain", payload).await?;
    let settings_for_probe = settings.clone();
    let public_url = public_url.to_owned();
    let probe = tokio::task::spawn_blocking(move || {
        if settings_for_probe.kind != MediaStorageKind::S3 {
            return Err(ApiError::bad_request(
                "Only S3-compatible storage can be tested.",
            ));
        }
        s3::probe_public_url(&public_url)
    })
    .await
    .map_err(|_| ApiError::service_unavailable("Media storage probe task failed."))?;
    let cleanup = delete_object(settings, key).await;
    match (probe, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// Writes one immutable object. Catalogue persistence happens afterward, so
/// callers must compensate with [`delete_object`] when that phase fails.
pub async fn put_object(
    settings: &MediaStorageSettings,
    key: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> Result<(), ApiError> {
    validate_storage_key(key)?;
    if bytes.is_empty() || bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(ApiError::bad_request(
            "Stored objects must be between 1 byte and 100 MiB.",
        ));
    }
    let settings = settings.clone();
    let key = key.to_owned();
    let content_type = content_type.to_owned();
    tokio::task::spawn_blocking(move || put_blocking(&settings, &key, &content_type, bytes))
        .await
        .map_err(|_| ApiError::service_unavailable("Media storage task failed."))?
}

pub async fn put_file(
    settings: &MediaStorageSettings,
    key: &str,
    content_type: &str,
    path: &std::path::Path,
    byte_size: u64,
    sha256: &str,
) -> Result<(), ApiError> {
    validate_storage_key(key)?;
    if byte_size == 0 || byte_size > MAX_ATTACHMENT_BYTES as u64 {
        return Err(ApiError::bad_request(
            "Stored objects must be between 1 byte and 100 MiB.",
        ));
    }
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ApiError::internal("Stored object checksum is invalid."));
    }
    let settings = settings.clone();
    let key = key.to_owned();
    let content_type = content_type.to_owned();
    let path = path.to_owned();
    let sha256 = sha256.to_ascii_lowercase();
    tokio::task::spawn_blocking(move || {
        put_file_blocking(&settings, &key, &content_type, &path, byte_size, &sha256)
    })
    .await
    .map_err(|_| ApiError::service_unavailable("Media storage task failed."))?
}

/// Deletes one object during upload compensation. Missing objects count as a
/// successful delete so retrying cleanup is always safe.
pub async fn delete_object(settings: &MediaStorageSettings, key: &str) -> Result<(), ApiError> {
    validate_storage_key(key)?;
    let settings = settings.clone();
    let key = key.to_owned();
    tokio::task::spawn_blocking(move || delete_blocking(&settings, &key))
        .await
        .map_err(|_| ApiError::service_unavailable("Media storage task failed."))?
}

fn put_blocking(
    settings: &MediaStorageSettings,
    key: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> Result<(), ApiError> {
    match settings.kind {
        MediaStorageKind::Local => write_local(settings, key, &bytes),
        MediaStorageKind::S3 => s3::put(settings, key, content_type, &bytes),
    }
}

fn put_file_blocking(
    settings: &MediaStorageSettings,
    key: &str,
    content_type: &str,
    path: &std::path::Path,
    byte_size: u64,
    sha256: &str,
) -> Result<(), ApiError> {
    match settings.kind {
        MediaStorageKind::Local => write_local_file(settings, key, path, byte_size),
        MediaStorageKind::S3 => s3::put_file(settings, key, content_type, path, byte_size, sha256),
    }
}

fn delete_blocking(settings: &MediaStorageSettings, key: &str) -> Result<(), ApiError> {
    match settings.kind {
        MediaStorageKind::Local => delete_local(settings, key),
        MediaStorageKind::S3 => s3::delete(settings, key),
    }
}

fn write_local(settings: &MediaStorageSettings, key: &str, bytes: &[u8]) -> Result<(), ApiError> {
    let path = local_path(settings, key)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            tracing::error!(%error, "media storage directory could not be created");
            ApiError::service_unavailable("Media storage is unavailable.")
        })?;
    }
    let temporary = path.with_extension(format!("{}.partial", Uuid::new_v4().simple()));
    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &path)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
        tracing::error!("media object write failed");
        return Err(ApiError::service_unavailable(
            "Media storage is unavailable.",
        ));
    }
    Ok(())
}

fn write_local_file(
    settings: &MediaStorageSettings,
    key: &str,
    source_path: &std::path::Path,
    expected_size: u64,
) -> Result<(), ApiError> {
    let path = local_path(settings, key)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(storage_write_error)?;
    }
    let temporary = path.with_extension(format!("{}.partial", Uuid::new_v4().simple()));
    let result = (|| -> std::io::Result<()> {
        let mut source = fs::File::open(source_path)?;
        let mut destination = fs::File::create(&temporary)?;
        let copied = std::io::copy(&mut source, &mut destination)?;
        if copied != expected_size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "staged object length changed before storage",
            ));
        }
        destination.sync_all()?;
        fs::rename(&temporary, &path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(storage_write_error(error));
    }
    Ok(())
}

fn storage_write_error(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "media object write failed");
    ApiError::service_unavailable("Media storage is unavailable.")
}

fn delete_local(settings: &MediaStorageSettings, key: &str) -> Result<(), ApiError> {
    let path = local_path(settings, key)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            tracing::error!(%error, storage_key = key, "media object deletion failed");
            Err(ApiError::service_unavailable(
                "Media storage cleanup failed.",
            ))
        }
    }
}

fn local_path(settings: &MediaStorageSettings, key: &str) -> Result<PathBuf, ApiError> {
    let root = settings.local_root.as_path();
    if root.as_os_str().is_empty() {
        return Err(ApiError::internal("Media storage root is not configured."));
    }
    let candidate = root.join(key);
    // `validate_storage_key` already rejects absolute keys and `.`/`..`
    // segments; the containment check keeps that invariant local to the file
    // backend as a second, independent guard.
    if candidate == root || !candidate.starts_with(root) {
        return Err(ApiError::internal("Media storage key escapes its root."));
    }
    Ok(candidate)
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
