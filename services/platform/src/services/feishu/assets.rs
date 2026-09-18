use std::path::Path;

use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    services::{media, object_storage_settings},
    state::AppState,
};

use super::{token_hash, SourceAttachment};

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

#[derive(Clone, Copy)]
struct AssetType {
    mime: &'static str,
    extension: &'static str,
    image: bool,
}

pub async fn store_source_assets(
    state: &AppState,
    connector_id: Uuid,
    attachments: &[SourceAttachment],
) -> Result<Vec<StoredSourceAsset>, ApiError> {
    let mut stored = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        match store_source_asset(state, connector_id, attachment).await {
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
    connector_id: Uuid,
    attachment: &SourceAttachment,
) -> Result<StoredSourceAsset, ApiError> {
    let source_token_hash = token_hash(&attachment.file_token);
    if let Some(asset) = load_binding(
        state,
        connector_id,
        &source_token_hash,
        &attachment.source_revision,
        attachment,
    )
    .await?
    {
        return Ok(asset);
    }
    let downloaded = state
        .feishu_client
        .download_asset(&attachment.file_token)
        .await?;
    if attachment
        .declared_size
        .is_some_and(|size| size != downloaded.bytes.len() as u64)
    {
        return Err(ApiError::conflict(format!(
            "Feishu attachment `{}` length differs from its metadata.",
            attachment.original_name
        )));
    }
    let asset_type = classify_asset(
        &attachment.original_name,
        &downloaded.bytes,
        downloaded.content_type.as_deref(),
        attachment.declared_media_type.as_deref(),
    )?;
    if asset_type.image && downloaded.bytes.len() > media::MAX_MEDIA_UPLOAD_BYTES {
        return Err(ApiError::new(
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            "Payload too large",
            "A Feishu image exceeds the 25 MiB image limit.",
        ));
    }
    if let Some(asset) = load_checksum_asset(
        state,
        connector_id,
        &source_token_hash,
        &attachment.source_revision,
        &downloaded.sha256,
        attachment,
    )
    .await?
    {
        return Ok(asset);
    }

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
        Some(media::generate_preview(downloaded.bytes.clone()).await?)
    } else {
        None
    };
    media::put_object(
        &settings,
        &storage_key,
        asset_type.mime,
        downloaded.bytes.clone(),
    )
    .await?;
    if let (Some(key), Some(preview)) = (preview_key.as_deref(), preview.as_ref()) {
        if let Err(error) =
            media::put_object(&settings, key, "image/webp", preview.bytes.clone()).await
        {
            let _ = media::delete_object(&settings, &storage_key).await;
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
        attachment,
        &source_token_hash,
        asset_id,
        &storage_key,
        preview_key.as_deref(),
        &public_url,
        preview_url.as_deref(),
        asset_type,
        &downloaded.sha256,
        downloaded.bytes.len() as i64,
        preview.as_ref(),
        settings.kind.label(),
    )
    .await;
    match persisted {
        Ok(()) => Ok(StoredSourceAsset {
            media_asset_id: asset_id,
            storage_key,
            preview_storage_key: preview_key,
            checksum: downloaded.sha256,
            media_type: asset_type.mime.into(),
            byte_size: downloaded.bytes.len() as i64,
            original_name: sanitize_name(&attachment.original_name),
            source_field_id: attachment.source_field_id.clone(),
            source_field_name: attachment.source_field_name.clone(),
            usage: attachment.usage.clone(),
            newly_created: true,
        }),
        Err(error) => {
            if let Some(key) = preview_key.as_deref() {
                let _ = media::delete_object(&settings, key).await;
            }
            let _ = media::delete_object(&settings, &storage_key).await;
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_new_asset(
    state: &AppState,
    connector_id: Uuid,
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
    token_hash: &str,
    revision: &str,
    asset_id: Uuid,
    checksum: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO feishu_asset_bindings
           (id,connector_id,source_token_hash,source_revision,media_asset_id,checksum)
           VALUES ($1,$2,$3,$4,$5,$6)
           ON CONFLICT (connector_id,source_token_hash,source_revision) DO NOTHING"#,
    )
    .bind(Uuid::new_v4())
    .bind(connector_id)
    .bind(token_hash)
    .bind(revision)
    .bind(asset_id)
    .bind(checksum)
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

fn classify_asset(
    name: &str,
    bytes: &[u8],
    response_type: Option<&str>,
    declared_type: Option<&str>,
) -> Result<AssetType, ApiError> {
    if let Some(image) = media::sniff_media_type(bytes) {
        return Ok(AssetType {
            mime: image.mime,
            extension: image.extension,
            image: true,
        });
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(unsupported_asset)?;
    let asset = document_type(&extension).ok_or_else(unsupported_asset)?;
    if !valid_signature(asset.extension, bytes) {
        return Err(ApiError::new(
            axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported media type",
            "The Feishu attachment content does not match its supported file type.",
        ));
    }
    for supplied in [response_type, declared_type].into_iter().flatten() {
        let supplied = supplied.split(';').next().unwrap_or(supplied).trim();
        if supplied != "application/octet-stream"
            && !supplied.eq_ignore_ascii_case(asset.mime)
            && !compatible_office_type(asset.extension, supplied)
        {
            return Err(ApiError::conflict(
                "Feishu attachment MIME metadata conflicts with its validated content.",
            ));
        }
    }
    Ok(asset)
}

fn document_type(extension: &str) -> Option<AssetType> {
    let (mime, extension) = match extension {
        "pdf" => ("application/pdf", "pdf"),
        "doc" => ("application/msword", "doc"),
        "docx" => (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
        ),
        "xls" => ("application/vnd.ms-excel", "xls"),
        "xlsx" => (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
        ),
        "dxf" => ("image/vnd.dxf", "dxf"),
        "dwg" => ("image/vnd.dwg", "dwg"),
        "step" => ("model/step", "step"),
        "stp" => ("model/step", "stp"),
        "iges" => ("model/iges", "iges"),
        "igs" => ("model/iges", "igs"),
        "stl" => ("model/stl", "stl"),
        "obj" => ("model/obj", "obj"),
        "3ds" => ("application/x-3ds", "3ds"),
        "sat" => ("application/octet-stream", "sat"),
        "prt" => ("application/octet-stream", "prt"),
        "sldprt" => ("application/octet-stream", "sldprt"),
        "asm" => ("application/octet-stream", "asm"),
        "sldasm" => ("application/octet-stream", "sldasm"),
        _ => return None,
    };
    Some(AssetType {
        mime,
        extension,
        image: false,
    })
}

fn valid_signature(extension: &str, bytes: &[u8]) -> bool {
    match extension {
        "pdf" => bytes.starts_with(b"%PDF-"),
        "doc" | "xls" => bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]),
        "docx" => zip_contains(bytes, b"word/"),
        "xlsx" => zip_contains(bytes, b"xl/"),
        "dwg" => bytes.starts_with(b"AC10"),
        "dxf" => bytes.starts_with(b"0\nSECTION") || bytes.starts_with(b"0\r\nSECTION"),
        "step" | "stp" => bytes.starts_with(b"ISO-10303-21"),
        "iges" | "igs" => bytes.len() >= 80,
        "stl" => bytes.starts_with(b"solid") || bytes.len() >= 84,
        "obj" => bytes.starts_with(b"#") || bytes.starts_with(b"v "),
        _ => !bytes.is_empty(),
    }
}

fn zip_contains(bytes: &[u8], path: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04") && bytes.windows(path.len()).any(|window| window == path)
}

fn compatible_office_type(extension: &str, supplied: &str) -> bool {
    matches!(extension, "docx" | "xlsx") && supplied == "application/zip"
}

fn unsupported_asset() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Unsupported media type",
        "Only PNG, JPEG, WebP, PDF, Word, Excel, and supported CAD originals are accepted.",
    )
}

fn sanitize_name(value: &str) -> String {
    let value = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .chars()
        .filter(|character| !character.is_control())
        .take(180)
        .collect::<String>();
    if value.trim().is_empty() {
        "feishu-attachment".into()
    } else {
        value
    }
}

#[cfg(test)]
#[path = "asset_tests.rs"]
mod tests;
