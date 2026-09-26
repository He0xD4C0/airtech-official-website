use axum::{extract::Multipart, http::StatusCode};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiError;
use crate::idempotency::{begin_for_subject, IdempotencyOutcome};
use crate::services::{cms_content::require_postgres, media_assets::decode_media_asset};
use crate::state::AppState;
use airtek_domain::models::MediaAsset;

use super::{generate_preview, storage, upload_input::stage_upload, validate_storage_key};

pub async fn upload_media_asset(
    state: &AppState,
    actor: &str,
    request_id: Uuid,
    idempotency_key: &str,
    multipart: Multipart,
) -> Result<MediaAsset, ApiError> {
    let staged = stage_upload(multipart).await?;
    let request_fingerprint = json!({
        "sha256": staged.sha256,
        "byteSize": staged.byte_size,
        "mediaType": staged.media_type.mime,
    });
    let idempotency = match begin_for_subject(
        state,
        "media.upload",
        actor,
        idempotency_key,
        &request_fingerprint,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => return replay.decode(),
        IdempotencyOutcome::Fresh(context) => context,
    };

    let settings = crate::services::object_storage_settings::active_storage(state).await?;
    let asset_id = Uuid::new_v4();
    let directory = format!(
        "{}/{}",
        settings.key_prefix.trim_matches('/'),
        Utc::now().format("%Y/%m")
    );
    let storage_key = format!(
        "{directory}/{}.{}",
        asset_id.simple(),
        staged.media_type.extension
    );
    let preview_storage_key = format!("{directory}/{}.preview.webp", asset_id.simple());
    validate_storage_key(&storage_key)?;
    validate_storage_key(&preview_storage_key)?;
    let bytes = tokio::fs::read(staged.path()).await.map_err(|error| {
        tracing::error!(%error, "staged media upload could not be read");
        ApiError::service_unavailable("Media upload staging is unavailable.")
    })?;
    let preview = generate_preview(bytes.clone()).await?;
    storage::put_object(&settings, &storage_key, staged.media_type.mime, bytes).await?;
    if let Err(error) = storage::put_object(
        &settings,
        &preview_storage_key,
        "image/webp",
        preview.bytes.clone(),
    )
    .await
    {
        compensate_objects(&settings, &[&storage_key]).await;
        return Err(error);
    }
    let public_url = crate::services::object_storage_settings::public_url(&settings, &storage_key);
    let preview_public_url =
        crate::services::object_storage_settings::public_url(&settings, &preview_storage_key);

    let persistence = persist_asset(
        state,
        idempotency,
        actor,
        request_id,
        asset_id,
        &storage_key,
        &public_url,
        &preview_storage_key,
        &preview_public_url,
        settings.kind.label(),
        &staged,
        &preview,
    )
    .await;
    match persistence {
        Ok(asset) => Ok(asset),
        Err(error) => {
            tracing::error!(%asset_id, persistence_error = %error, "media upload persistence failed");
            compensate_objects(&settings, &[&preview_storage_key, &storage_key]).await;
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_asset(
    state: &AppState,
    idempotency: crate::idempotency::IdempotencyContext,
    actor: &str,
    request_id: Uuid,
    asset_id: Uuid,
    storage_key: &str,
    public_url: &str,
    preview_storage_key: &str,
    preview_public_url: &str,
    storage_backend: &str,
    staged: &super::upload_input::StagedUpload,
    preview: &super::PreviewDerivative,
) -> Result<MediaAsset, ApiError> {
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,public_url,preview_storage_key,preview_public_url,
            original_name,media_type,byte_size,original_width,original_height,
            preview_width,preview_height,preview_media_type,preview_byte_size,checksum,
            metadata,created_at,storage_backend,
            content_type,uploaded_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'image/webp',$13,$14,
                   '{}'::jsonb,now(),$15,$7,$16)
           RETURNING id,public_url,preview_public_url,original_name,media_type,byte_size,
                     original_width,original_height,preview_width,preview_height,
                     preview_media_type,preview_byte_size,checksum,uploaded_by,created_at"#,
    )
    .bind(asset_id)
    .bind(storage_key)
    .bind(public_url)
    .bind(preview_storage_key)
    .bind(preview_public_url)
    .bind(&staged.file_name)
    .bind(staged.media_type.mime)
    .bind(staged.byte_size as i64)
    .bind(preview.original_width as i32)
    .bind(preview.original_height as i32)
    .bind(preview.width as i32)
    .bind(preview.height as i32)
    .bind(preview.bytes.len() as i64)
    .bind(&staged.sha256)
    .bind(storage_backend)
    .bind(actor)
    .fetch_one(&mut *transaction)
    .await?;
    let asset = decode_media_asset(&row)?;
    insert_audit(
        &mut transaction,
        actor,
        request_id,
        asset_id,
        json!({
            "originalName": asset.original_name,
            "mediaType": asset.media_type,
            "byteSize": asset.byte_size,
            "sha256": asset.sha256,
            "publicUrl": asset.public_url,
            "previewUrl": asset.preview_url,
            "originalWidth": asset.original_width,
            "originalHeight": asset.original_height,
            "previewWidth": asset.preview_width,
            "previewHeight": asset.preview_height,
            "previewByteSize": asset.preview_byte_size,
        }),
    )
    .await?;
    let staged_idempotency = idempotency
        .stage_in_transaction(&mut transaction, &asset, StatusCode::CREATED)
        .await?;
    transaction.commit().await?;
    staged_idempotency.finish().await?;
    Ok(asset)
}

async fn compensate_objects(settings: &super::MediaStorageSettings, keys: &[&str]) {
    for key in keys {
        if let Err(error) = storage::delete_object(settings, key).await {
            tracing::error!(storage_key = *key, %error, "media upload compensation failed");
        }
    }
}

async fn insert_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &str,
    request_id: Uuid,
    asset_id: Uuid,
    after: serde_json::Value,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,'media.upload','media',$3,NULL,$4,
                   'Upload a public media asset',$5,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(asset_id)
    .bind(after)
    .bind(request_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
