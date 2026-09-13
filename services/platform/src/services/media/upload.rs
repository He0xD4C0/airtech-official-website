use axum::{extract::Multipart, http::StatusCode};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    error::ApiError,
    idempotency::{begin_for_subject, IdempotencyOutcome},
    models::MediaAsset,
    services::{cms_content::require_postgres, media_assets::decode_media_asset},
    state::AppState,
};

use super::{storage, upload_input::stage_upload, validate_storage_key};

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

    let settings = state.config.media.storage.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(
            "Object storage is not configured; media uploads are disabled.",
        )
    })?;
    let asset_id = Uuid::new_v4();
    let storage_key = format!(
        "{}/{}/{}.{}",
        settings.key_prefix.trim_matches('/'),
        Utc::now().format("%Y/%m"),
        asset_id.simple(),
        staged.media_type.extension
    );
    validate_storage_key(&storage_key)?;
    let bytes = tokio::fs::read(staged.path()).await.map_err(|error| {
        tracing::error!(%error, "staged media upload could not be read");
        ApiError::service_unavailable("Media upload staging is unavailable.")
    })?;
    storage::put_object(settings, &storage_key, staged.media_type.mime, bytes).await?;

    let persistence = persist_asset(
        state,
        idempotency,
        actor,
        request_id,
        asset_id,
        &storage_key,
        &staged,
    )
    .await;
    match persistence {
        Ok(asset) => Ok(asset),
        Err(error) => {
            if let Err(cleanup_error) = storage::delete_object(settings, &storage_key).await {
                tracing::error!(
                    %asset_id,
                    %storage_key,
                    persistence_error = %error,
                    cleanup_error = %cleanup_error,
                    "media upload compensation failed"
                );
            }
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
    staged: &super::upload_input::StagedUpload,
) -> Result<MediaAsset, ApiError> {
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,original_name,media_type,byte_size,checksum,
            metadata,created_at,storage_backend,
            content_type,uploaded_by)
           VALUES ($1,$2,$3,$4,$5,$6,'{}'::jsonb,now(),$7,$4,$8)
           RETURNING id,original_name,media_type,byte_size,checksum,uploaded_by,created_at"#,
    )
    .bind(asset_id)
    .bind(storage_key)
    .bind(&staged.file_name)
    .bind(staged.media_type.mime)
    .bind(staged.byte_size as i64)
    .bind(&staged.sha256)
    .bind(
        state
            .config
            .media
            .storage
            .as_ref()
            .expect("storage checked before persistence")
            .kind
            .label(),
    )
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
