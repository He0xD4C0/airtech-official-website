use std::path::Path;

use sha2::Digest;
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::{media, object_storage_settings};
use crate::state::AppState;

use super::asset_types::{classify_asset, sanitize_name, staged_attachment_error};
use super::{persist_new_asset, StoredSourceAsset};
use crate::services::feishu::client::AssetProbe;
use crate::services::feishu::{token_hash, SourceAttachment};

pub async fn store_archive_asset(
    state: &AppState,
    connector_id: Uuid,
    sync_run_id: Uuid,
    attachment: &SourceAttachment,
    path: &Path,
    expected_sha256: &str,
    expected_size: i64,
) -> Result<StoredSourceAsset, ApiError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(staged_attachment_error)?;
    if metadata.len() as i64 != expected_size {
        return Err(ApiError::conflict(
            "Archived object size does not match its manifest.",
        ));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(staged_attachment_error)?;
    let actual = format!("{:x}", sha2::Sha256::digest(&bytes));
    if actual != expected_sha256 {
        return Err(ApiError::conflict(
            "Archived object checksum does not match its manifest.",
        ));
    }
    let asset_type = classify_asset(
        &attachment.original_name,
        &AssetProbe::from_bytes(&bytes),
        metadata.len(),
        None,
        attachment.declared_media_type.as_deref(),
    )?;
    let settings = object_storage_settings::active_storage(state).await?;
    let token_hash = token_hash(&attachment.file_token);
    let key_base = format!(
        "{}/archive/{}/{}-{}",
        settings.key_prefix.trim_matches('/'),
        &token_hash[..2],
        token_hash,
        expected_sha256
    );
    let storage_key = format!("{key_base}.{}", asset_type.extension);
    let preview_key = asset_type.image.then(|| format!("{key_base}.preview.webp"));
    let preview = if asset_type.image {
        Some(media::generate_preview(bytes.clone()).await?)
    } else {
        None
    };
    media::put_file(
        &settings,
        &storage_key,
        asset_type.mime,
        path,
        metadata.len(),
        expected_sha256,
    )
    .await?;
    if let Some(preview) = &preview {
        media::put_object(
            &settings,
            preview_key.as_deref().unwrap(),
            "image/webp",
            preview.bytes.clone(),
        )
        .await?;
    }
    let public_url = object_storage_settings::public_url(&settings, &storage_key);
    let preview_url = preview_key
        .as_deref()
        .map(|key| object_storage_settings::public_url(&settings, key));
    let asset_id = Uuid::new_v4();
    persist_new_asset(
        state,
        connector_id,
        sync_run_id,
        attachment,
        &token_hash,
        asset_id,
        &storage_key,
        preview_key.as_deref(),
        &public_url,
        preview_url.as_deref(),
        asset_type,
        expected_sha256,
        metadata.len() as i64,
        preview.as_ref(),
        settings.kind.label(),
    )
    .await?;
    Ok(StoredSourceAsset {
        media_asset_id: asset_id,
        storage_key,
        preview_storage_key: preview_key,
        checksum: expected_sha256.to_owned(),
        media_type: asset_type.mime.to_owned(),
        byte_size: metadata.len() as i64,
        original_name: sanitize_name(&attachment.original_name),
        source_field_id: attachment.source_field_id.clone(),
        source_field_name: attachment.source_field_name.clone(),
        usage: attachment.usage.clone(),
        newly_created: true,
    })
}
