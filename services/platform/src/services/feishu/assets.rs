use serde_json::{json, Map, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    services::{media, object_storage_settings},
    state::AppState,
};

use super::{client::AssetProbe, token_hash, FeishuClient, SourceAttachment};

#[path = "asset_types.rs"]
mod asset_types;
use asset_types::{classify_asset, sanitize_name, staged_attachment_error, AssetType};

#[derive(Clone, Debug)]
pub struct StoredSourceAsset {
    pub media_asset_id: Uuid,
    pub storage_key: String,
    pub preview_storage_key: Option<String>,
    pub checksum: String,
    pub media_type: String,
    pub byte_size: i64,
    pub original_name: String,
    pub source_field_id: String,
    pub source_field_name: String,
    pub usage: String,
    pub newly_created: bool,
}

pub async fn store_source_assets(
    state: &AppState,
    client: &FeishuClient,
    connector_id: Uuid,
    sync_run_id: Uuid,
    attachments: &[SourceAttachment],
) -> Result<Vec<StoredSourceAsset>, ApiError> {
    let mut stored = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        match store_source_asset(state, client, connector_id, sync_run_id, attachment).await {
            Ok(asset) => stored.push(asset),
            Err(error) => {
                super::compensate_source_assets(state, &stored).await;
                return Err(error);
            }
        }
    }
    Ok(stored)
}

async fn store_source_asset(
    state: &AppState,
    client: &FeishuClient,
    connector_id: Uuid,
    sync_run_id: Uuid,
    attachment: &SourceAttachment,
) -> Result<StoredSourceAsset, ApiError> {
    let source_token_hash = token_hash(&attachment.file_token);
    if let Some(asset) = load_binding(
        state,
        connector_id,
        sync_run_id,
        &source_token_hash,
        &attachment.source_revision,
        attachment,
    )
    .await?
    {
        return Ok(asset);
    }
    let extra = attachment_permission_extra(attachment);
    let downloaded = client
        .download_asset(&attachment.file_token, Some(&extra))
        .await?;
    if attachment
        .declared_size
        .is_some_and(|size| size != downloaded.byte_size)
    {
        return Err(ApiError::conflict(format!(
            "Feishu attachment `{}` length differs from its metadata.",
            attachment.original_name
        )));
    }
    let asset_type = classify_asset(
        &attachment.original_name,
        downloaded.probe(),
        downloaded.byte_size,
        downloaded.content_type.as_deref(),
        attachment.declared_media_type.as_deref(),
    )?;
    if asset_type.image && downloaded.byte_size > media::MAX_MEDIA_UPLOAD_BYTES as u64 {
        return Err(ApiError::new(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            "Payload too large",
            "A Feishu image exceeds the 25 MiB image limit.",
        ));
    }
    if let Some(asset) = load_checksum_asset(
        state,
        connector_id,
        sync_run_id,
        &source_token_hash,
        &attachment.source_revision,
        &downloaded.sha256,
        attachment,
    )
    .await?
    {
        return Ok(asset);
    }
    let byte_size = downloaded.byte_size as i64;

    let settings = object_storage_settings::active_storage(state).await?;
    let asset_id = Uuid::new_v4();
    let key_base = format!(
        "{}/feishu/{}/{}-{}",
        settings.key_prefix.trim_matches('/'),
        &source_token_hash[..2],
        source_token_hash,
        downloaded.sha256
    );
    let storage_key = format!("{key_base}.{}", asset_type.extension);
    let preview_key = asset_type.image.then(|| format!("{key_base}.preview.webp"));
    let preview = if asset_type.image {
        let bytes = tokio::fs::read(downloaded.path())
            .await
            .map_err(staged_attachment_error)?;
        Some(media::generate_preview(bytes).await?)
    } else {
        None
    };
    media::put_file(
        &settings,
        &storage_key,
        asset_type.mime,
        downloaded.path(),
        downloaded.byte_size,
        &downloaded.sha256,
    )
    .await?;
    if let (Some(key), Some(preview)) = (preview_key.as_deref(), preview.as_ref()) {
        if let Err(error) =
            media::put_object(&settings, key, "image/webp", preview.bytes.clone()).await
        {
            defer_or_delete_orphan(
                state,
                &settings,
                &storage_key,
                preview_key.as_deref(),
                "previewUploadFailed",
            )
            .await;
            return Err(error);
        }
    }
    let public_url = object_storage_settings::public_url(&settings, &storage_key);
    let preview_url = preview_key
        .as_deref()
        .map(|key| object_storage_settings::public_url(&settings, key));
    let persisted = persist_new_asset(
        state,
        connector_id,
        sync_run_id,
        attachment,
        &source_token_hash,
        asset_id,
        &storage_key,
        preview_key.as_deref(),
        &public_url,
        preview_url.as_deref(),
        asset_type,
        &downloaded.sha256,
        byte_size,
        preview.as_ref(),
        settings.kind.label(),
    )
    .await;
    match persisted {
        Ok(()) => Ok(StoredSourceAsset {
            media_asset_id: asset_id,
            storage_key,
            preview_storage_key: preview_key,
            checksum: downloaded.sha256.clone(),
            media_type: asset_type.mime.into(),
            byte_size,
            original_name: sanitize_name(&attachment.original_name),
            source_field_id: attachment.source_field_id.clone(),
            source_field_name: attachment.source_field_name.clone(),
            usage: attachment.usage.clone(),
            newly_created: true,
        }),
        Err(error) => {
            defer_or_delete_orphan(
                state,
                &settings,
                &storage_key,
                preview_key.as_deref(),
                "catalogueCommitFailed",
            )
            .await;
            Err(error)
        }
    }
}

async fn defer_or_delete_orphan(
    state: &AppState,
    settings: &media::MediaStorageSettings,
    storage_key: &str,
    preview_key: Option<&str>,
    reason: &str,
) {
    if let Err(error) =
        super::queue_orphan_object_cleanup(state, storage_key, preview_key, reason).await
    {
        tracing::error!(%error, storage_key, "Feishu orphan cleanup could not be persisted");
        if let Some(key) = preview_key {
            let _ = media::delete_object(settings, key).await;
        }
        let _ = media::delete_object(settings, storage_key).await;
    }
}

fn attachment_permission_extra(attachment: &SourceAttachment) -> String {
    let mut records = Map::new();
    records.insert(
        attachment.source_record_id.clone(),
        json!([attachment.file_token]),
    );
    let mut fields = Map::new();
    fields.insert(attachment.source_field_id.clone(), Value::Object(records));
    json!({
        "bitablePerm": {
            "tableId": attachment.table_id,
            "attachments": Value::Object(fields)
        }
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
async fn persist_new_asset(
    state: &AppState,
    connector_id: Uuid,
    sync_run_id: Uuid,
    attachment: &SourceAttachment,
    source_token_hash: &str,
    asset_id: Uuid,
    storage_key: &str,
    preview_key: Option<&str>,
    public_url: &str,
    preview_url: Option<&str>,
    asset_type: AssetType,
    checksum: &str,
    byte_size: i64,
    preview: Option<&media::PreviewDerivative>,
    backend: &str,
) -> Result<(), ApiError> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,public_url,preview_storage_key,preview_public_url,
            original_name,media_type,byte_size,original_width,original_height,
            preview_width,preview_height,preview_media_type,preview_byte_size,
            checksum,metadata,created_at,storage_backend,content_type,uploaded_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                   now(),$17,$7,'feishuSync')"#,
    )
    .bind(asset_id)
    .bind(storage_key)
    .bind(public_url)
    .bind(preview_key)
    .bind(preview_url)
    .bind(sanitize_name(&attachment.original_name))
    .bind(asset_type.mime)
    .bind(byte_size)
    .bind(preview.map(|value| value.original_width as i32))
    .bind(preview.map(|value| value.original_height as i32))
    .bind(preview.map(|value| value.width as i32))
    .bind(preview.map(|value| value.height as i32))
    .bind(preview.map(|_| "image/webp"))
    .bind(preview.map(|value| value.bytes.len() as i64))
    .bind(checksum)
    .bind(json!({
        "source": "feishu",
        "sourceFieldId": attachment.source_field_id,
        "sourceFieldName": attachment.source_field_name
    }))
    .bind(backend)
    .execute(&mut *transaction)
    .await?;
    insert_binding(
        &mut transaction,
        connector_id,
        sync_run_id,
        source_token_hash,
        &attachment.source_revision,
        asset_id,
        checksum,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn load_binding(
    state: &AppState,
    connector_id: Uuid,
    _sync_run_id: Uuid,
    token_hash: &str,
    revision: &str,
    attachment: &SourceAttachment,
) -> Result<Option<StoredSourceAsset>, ApiError> {
    let row = sqlx::query(
        r#"SELECT media.id,media.storage_key,media.preview_storage_key,media.checksum,
                  media.media_type,media.byte_size,media.original_name
           FROM feishu_asset_bindings binding
           JOIN media_assets media ON media.id=binding.media_asset_id
           WHERE binding.connector_id=$1 AND binding.source_token_hash=$2
             AND binding.source_revision=$3 AND media.deleted_at IS NULL
             AND media.scan_status='clean' AND media.access_level='public'"#,
    )
    .bind(connector_id)
    .bind(token_hash)
    .bind(revision)
    .fetch_optional(&state.pool)
    .await?;
    row.map(|row| decode_stored(&row, attachment, false))
        .transpose()
}

async fn load_checksum_asset(
    state: &AppState,
    connector_id: Uuid,
    sync_run_id: Uuid,
    token_hash: &str,
    revision: &str,
    checksum: &str,
    attachment: &SourceAttachment,
) -> Result<Option<StoredSourceAsset>, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,storage_key,preview_storage_key,checksum,media_type,byte_size,original_name
           FROM media_assets WHERE checksum=$1 AND deleted_at IS NULL
             AND scan_status='clean' AND access_level='public'
           ORDER BY created_at,id LIMIT 1"#,
    )
    .bind(checksum)
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let asset_id: Uuid = row.try_get("id")?;
    let mut transaction = state.pool.begin().await?;
    insert_binding(
        &mut transaction,
        connector_id,
        sync_run_id,
        token_hash,
        revision,
        asset_id,
        checksum,
    )
    .await?;
    transaction.commit().await?;
    decode_stored(&row, attachment, false).map(Some)
}

async fn insert_binding(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    connector_id: Uuid,
    sync_run_id: Uuid,
    token_hash: &str,
    revision: &str,
    asset_id: Uuid,
    checksum: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO feishu_asset_bindings
           (id,connector_id,source_token_hash,source_revision,media_asset_id,checksum,
            created_by_sync_run_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7)
           ON CONFLICT (connector_id,source_token_hash,source_revision) DO NOTHING"#,
    )
    .bind(Uuid::new_v4())
    .bind(connector_id)
    .bind(token_hash)
    .bind(revision)
    .bind(asset_id)
    .bind(checksum)
    .bind(sync_run_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn decode_stored(
    row: &sqlx::postgres::PgRow,
    attachment: &SourceAttachment,
    newly_created: bool,
) -> Result<StoredSourceAsset, ApiError> {
    Ok(StoredSourceAsset {
        media_asset_id: row.try_get("id")?,
        storage_key: row.try_get("storage_key")?,
        preview_storage_key: row.try_get("preview_storage_key")?,
        checksum: row.try_get("checksum")?,
        media_type: row.try_get("media_type")?,
        byte_size: row.try_get("byte_size")?,
        original_name: row.try_get("original_name")?,
        source_field_id: attachment.source_field_id.clone(),
        source_field_name: attachment.source_field_name.clone(),
        usage: attachment.usage.clone(),
        newly_created,
    })
}

#[cfg(test)]
#[path = "asset_tests.rs"]
mod tests;
