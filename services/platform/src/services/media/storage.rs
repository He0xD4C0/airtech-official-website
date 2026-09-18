use std::{fs, io::Write, path::PathBuf};

use axum::body::{Body, Bytes};
use futures_util::stream;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::error::ApiError;

use super::config::{MediaStorageKind, MediaStorageSettings};
use super::s3;
use super::{validate_storage_key, MAX_ATTACHMENT_BYTES};

#[allow(dead_code)]
pub struct MediaObject {
    pub body: Body,
    pub content_length: u64,
}

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

pub async fn get_object(
    settings: &MediaStorageSettings,
    key: &str,
    expected_size: u64,
) -> Result<MediaObject, ApiError> {
    validate_storage_key(key)?;
    if expected_size > MAX_ATTACHMENT_BYTES as u64 {
        return Err(ApiError::service_unavailable(
            "Stored media object exceeds the delivery limit.",
        ));
    }
    match settings.kind {
        MediaStorageKind::Local => read_local(settings, key, expected_size).await,
        MediaStorageKind::S3 => s3::get(settings, key, expected_size).await,
    }
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

#[allow(dead_code)]
async fn read_local(
    settings: &MediaStorageSettings,
    key: &str,
    expected_size: u64,
) -> Result<MediaObject, ApiError> {
    let path = local_path(settings, key)?;
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ApiError::not_found("Media object was not found."))
        }
        Err(error) => {
            tracing::error!(%error, "media object open failed");
            return Err(ApiError::service_unavailable(
                "Media storage is unavailable.",
            ));
        }
    };
    let metadata = match file.metadata().await {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::error!(%error, "media object metadata failed");
            return Err(ApiError::service_unavailable(
                "Media storage is unavailable.",
            ));
        }
    };
    if metadata.len() != expected_size {
        tracing::error!(
            storage_key = key,
            expected_size,
            actual_size = metadata.len(),
            "media object length differs from catalogue metadata"
        );
        return Err(ApiError::service_unavailable(
            "Stored media object does not match its catalogue metadata.",
        ));
    }
    let chunks = stream::try_unfold((file, expected_size), |(mut file, remaining)| async move {
        if remaining == 0 {
            return Ok(None);
        }
        let capacity = usize::try_from(remaining.min(64 * 1024)).unwrap_or(64 * 1024);
        let mut buffer = vec![0_u8; capacity];
        let read = file.read(&mut buffer).await.map_err(|error| {
            tracing::error!(%error, "media object stream read failed");
            error
        })?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "media object ended before its catalogue length",
            ));
        }
        buffer.truncate(read);
        Ok(Some((Bytes::from(buffer), (file, remaining - read as u64))))
    });
    Ok(MediaObject {
        body: Body::from_stream(chunks),
        content_length: expected_size,
    })
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
