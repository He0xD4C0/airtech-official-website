use std::collections::BTreeSet;

use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::{media, object_storage_settings};
use crate::state::AppState;

use super::StoredSourceAsset;

/// Compensate uploaded objects without losing retry evidence. A failed object
/// delete leaves the catalogue row intact and queues a durable cleanup job.
pub async fn compensate_source_assets(state: &AppState, assets: &[StoredSourceAsset]) {
    let created = assets
        .iter()
        .filter(|asset| asset.newly_created)
        .map(|asset| asset.media_asset_id)
        .collect::<BTreeSet<_>>();
    for asset_id in created {
        match queue_asset_cleanup(state, asset_id, "uploadCompensation").await {
            Ok(Some(compensation_id)) => {
                if let Err(error) = cleanup_once(state, compensation_id).await {
                    tracing::error!(%error, %asset_id, %compensation_id, "Feishu upload compensation deferred");
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, %asset_id, "Feishu compensation could not be queued")
            }
        }
    }
}

pub async fn execute_object_cleanup(
    state: &AppState,
    compensation_id: Uuid,
) -> Result<Value, String> {
    match cleanup_once(state, compensation_id).await {
        Ok(deleted) => Ok(json!({"compensationId": compensation_id, "deleted": deleted})),
        Err(error) => {
            let _ = sqlx::query(
                r#"UPDATE feishu_object_compensations
                   SET attempts=attempts+1,last_error=$2,updated_at=now()
                   WHERE id=$1"#,
            )
            .bind(compensation_id)
            .bind(error.to_string())
            .execute(&state.pool)
            .await;
            Err(error.to_string())
        }
    }
}

pub fn parse_object_cleanup_payload(payload: &Value) -> Result<Uuid, String> {
    let object = payload.as_object().ok_or_else(|| {
        "Feishu object cleanup payload must contain only compensationId.".to_owned()
    })?;
    if object.len() != 1 {
        return Err("Feishu object cleanup payload must contain only compensationId.".into());
    }
    object
        .get("compensationId")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| "Feishu object cleanup compensationId is invalid.".into())
}

async fn cleanup_once(state: &AppState, compensation_id: Uuid) -> Result<bool, ApiError> {
    let settings = object_storage_settings::active_storage(state).await?;
    let mut transaction = state.pool.begin().await?;
    let row = sqlx::query(
        r#"SELECT media_asset_id,storage_key,preview_storage_key
           FROM feishu_object_compensations WHERE id=$1 FOR UPDATE"#,
    )
    .bind(compensation_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(row) = row else {
        transaction.rollback().await?;
        return Ok(false);
    };
    let asset_id: Option<Uuid> = row.try_get("media_asset_id")?;
    let storage_key: String = row.try_get("storage_key")?;
    let preview_key: Option<String> = row.try_get("preview_storage_key")?;
    if let Some(asset_id) = asset_id {
        let _locked = sqlx::query("SELECT id FROM media_assets WHERE id=$1 FOR UPDATE")
            .bind(asset_id)
            .fetch_optional(&mut *transaction)
            .await?;
        if asset_is_referenced(&mut transaction, asset_id).await? {
            sqlx::query("DELETE FROM feishu_object_compensations WHERE id=$1")
                .bind(compensation_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            return Ok(false);
        }
    } else if catalogue_owns_key(&mut transaction, &storage_key, preview_key.as_deref()).await? {
        sqlx::query("DELETE FROM feishu_object_compensations WHERE id=$1")
            .bind(compensation_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        return Ok(false);
    }
    if let Some(key) = preview_key.as_deref() {
        media::delete_object(&settings, key).await?;
    }
    media::delete_object(&settings, &storage_key).await?;
    if let Some(asset_id) = asset_id {
        sqlx::query("DELETE FROM feishu_asset_bindings WHERE media_asset_id=$1")
            .bind(asset_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM media_assets WHERE id=$1")
            .bind(asset_id)
            .execute(&mut *transaction)
            .await?;
    }
    sqlx::query("DELETE FROM feishu_object_compensations WHERE id=$1")
        .bind(compensation_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(true)
}

async fn catalogue_owns_key(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    storage_key: &str,
    preview_key: Option<&str>,
) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM media_assets
           WHERE deleted_at IS NULL
             AND (storage_key=$1 OR ($2::text IS NOT NULL AND preview_storage_key=$2)))"#,
    )
    .bind(storage_key)
    .bind(preview_key)
    .fetch_one(&mut **transaction)
    .await?)
}

async fn asset_is_referenced(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    asset_id: Uuid,
) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM asset_references WHERE media_asset_id=$1)
           OR EXISTS(SELECT 1 FROM cms_current_publication_dependencies
                     WHERE target_media_asset_id=$1)
           OR EXISTS(SELECT 1 FROM product_presentation_working
                     WHERE content->'mediaGallery' @>
                       jsonb_build_array(jsonb_build_object('assetId',$1::text)))
           OR EXISTS(SELECT 1 FROM cms_drafts
                     WHERE jsonb_path_exists(
                       document,'$.**.assetId ? (@ == $asset)',
                       jsonb_build_object('asset',to_jsonb($1::text))))
           OR EXISTS(SELECT 1 FROM product_import_missing_assets
                     WHERE resolved_media_asset_id=$1)"#,
    )
    .bind(asset_id)
    .fetch_one(&mut **transaction)
    .await?)
}

async fn queue_asset_cleanup(
    state: &AppState,
    asset_id: Uuid,
    reason: &str,
) -> Result<Option<Uuid>, ApiError> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("feishu:asset-cleanup:{asset_id}"))
        .execute(&mut *transaction)
        .await?;
    let row = sqlx::query(
        "SELECT storage_key,preview_storage_key FROM media_assets WHERE id=$1 FOR UPDATE",
    )
    .bind(asset_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(row) = row else {
        transaction.rollback().await?;
        return Ok(None);
    };
    if asset_is_referenced(&mut transaction, asset_id).await? {
        transaction.rollback().await?;
        return Ok(None);
    }
    let id = insert_cleanup_intent(
        &mut transaction,
        Some(asset_id),
        &row.try_get::<String, _>("storage_key")?,
        row.try_get::<Option<String>, _>("preview_storage_key")?
            .as_deref(),
        reason,
    )
    .await?;
    transaction.commit().await?;
    Ok(Some(id))
}

pub async fn queue_orphan_object_cleanup(
    state: &AppState,
    storage_key: &str,
    preview_key: Option<&str>,
    reason: &str,
) -> Result<Uuid, ApiError> {
    let mut transaction = state.pool.begin().await?;
    let id =
        insert_cleanup_intent(&mut transaction, None, storage_key, preview_key, reason).await?;
    transaction.commit().await?;
    Ok(id)
}

pub(super) async fn queue_purge_asset_cleanup(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    asset_ids: &[Uuid],
    deleting_product_ids: &[Uuid],
) -> Result<(), ApiError> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query(
        r#"SELECT media.id,media.storage_key,media.preview_storage_key
           FROM media_assets media WHERE media.id=ANY($1)
             AND NOT EXISTS(
               SELECT 1 FROM asset_references reference
               WHERE reference.media_asset_id=media.id
                 AND (reference.product_id IS NULL
                      OR NOT (reference.product_id=ANY($2))))
             AND NOT EXISTS(SELECT 1 FROM cms_current_publication_dependencies
                            WHERE target_media_asset_id=media.id)
             AND NOT EXISTS(
               SELECT 1 FROM product_presentation_working presentation
               WHERE NOT (presentation.product_id=ANY($2))
                 AND presentation.content->'mediaGallery' @>
                   jsonb_build_array(jsonb_build_object('assetId',media.id::text)))
             AND NOT EXISTS(
               SELECT 1 FROM cms_drafts draft
               WHERE jsonb_path_exists(
                 draft.document,'$.**.assetId ? (@ == $asset)',
                 jsonb_build_object('asset',to_jsonb(media.id::text))))
             AND NOT EXISTS(SELECT 1 FROM product_import_missing_assets
                            WHERE resolved_media_asset_id=media.id)
           FOR UPDATE OF media"#,
    )
    .bind(asset_ids)
    .bind(deleting_product_ids)
    .fetch_all(&mut **transaction)
    .await?;
    for row in rows {
        insert_cleanup_intent(
            transaction,
            Some(row.try_get("id")?),
            &row.try_get::<String, _>("storage_key")?,
            row.try_get::<Option<String>, _>("preview_storage_key")?
                .as_deref(),
            "sourcePurge",
        )
        .await?;
    }
    Ok(())
}

async fn insert_cleanup_intent(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    asset_id: Option<Uuid>,
    storage_key: &str,
    preview_key: Option<&str>,
    reason: &str,
) -> Result<Uuid, ApiError> {
    let orphan = asset_id.is_none();
    let id = Uuid::new_v4();
    let compensation_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO feishu_object_compensations
           (id,media_asset_id,storage_key,preview_storage_key,reason)
           VALUES ($1,$2,$3,$4,$5)
           ON CONFLICT(media_asset_id) DO UPDATE SET
             storage_key=EXCLUDED.storage_key,
             preview_storage_key=EXCLUDED.preview_storage_key,
             reason=EXCLUDED.reason,updated_at=now()
           RETURNING id"#,
    )
    .bind(id)
    .bind(asset_id)
    .bind(storage_key)
    .bind(preview_key)
    .bind(reason)
    .fetch_one(&mut **transaction)
    .await?;
    let active: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM jobs
           WHERE job_type='feishuObjectCleanup' AND status IN ('queued','running')
             AND payload->>'compensationId'=$1::text)"#,
    )
    .bind(compensation_id)
    .fetch_one(&mut **transaction)
    .await?;
    if !active {
        let job_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO operation_runs(id,kind,status,reason,result,created_at,updated_at)
               VALUES ($1,'feishuSync','queued',$2,NULL,now(),now())"#,
        )
        .bind(job_id)
        .bind(format!(
            "Remove unreferenced Feishu object {compensation_id}"
        ))
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO jobs(id,job_type,status,payload,available_at,created_at,updated_at)
               VALUES ($1,'feishuObjectCleanup','queued',$2,
                       CASE WHEN $3 THEN now()+interval '15 minutes' ELSE now() END,
                       now(),now())"#,
        )
        .bind(job_id)
        .bind(json!({"compensationId": compensation_id}))
        .bind(orphan)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(compensation_id)
}
