use std::collections::BTreeSet;

use uuid::Uuid;

use crate::{
    services::{media, object_storage_settings},
    state::AppState,
};

use super::StoredSourceAsset;

/// Best-effort compensation for objects created by a record that could not be
/// published. Reused assets and assets already referenced elsewhere are kept.
pub async fn compensate_source_assets(state: &AppState, assets: &[StoredSourceAsset]) {
    let created = assets
        .iter()
        .filter(|asset| asset.newly_created)
        .map(|asset| asset.media_asset_id)
        .collect::<BTreeSet<_>>();
    if created.is_empty() {
        return;
    }
    let settings = match object_storage_settings::active_storage(state).await {
        Ok(settings) => settings,
        Err(error) => {
            tracing::error!(%error, "Feishu asset compensation could not load storage settings");
            return;
        }
    };
    for asset_id in created {
        let Some(asset) = assets.iter().find(|asset| asset.media_asset_id == asset_id) else {
            continue;
        };
        match detach_unreferenced_asset(state, asset_id).await {
            Ok(true) => {
                if let Some(key) = asset.preview_storage_key.as_deref() {
                    if let Err(error) = media::delete_object(&settings, key).await {
                        tracing::error!(%error, %asset_id, "Feishu preview compensation failed");
                    }
                }
                if let Err(error) = media::delete_object(&settings, &asset.storage_key).await {
                    tracing::error!(%error, %asset_id, "Feishu original compensation failed");
                }
            }
            Ok(false) => {}
            Err(error) => {
                tracing::error!(%error, %asset_id, "Feishu asset compensation transaction failed");
            }
        }
    }
}

async fn detach_unreferenced_asset(state: &AppState, asset_id: Uuid) -> Result<bool, sqlx::Error> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT id FROM media_assets WHERE id=$1 FOR UPDATE")
        .bind(asset_id)
        .fetch_optional(&mut *transaction)
        .await?;
    let referenced: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM asset_references WHERE media_asset_id=$1)")
            .bind(asset_id)
            .fetch_one(&mut *transaction)
            .await?;
    if referenced {
        transaction.rollback().await?;
        return Ok(false);
    }
    sqlx::query("DELETE FROM feishu_asset_bindings WHERE media_asset_id=$1")
        .bind(asset_id)
        .execute(&mut *transaction)
        .await?;
    let deleted = sqlx::query("DELETE FROM media_assets WHERE id=$1")
        .bind(asset_id)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
        == 1;
    transaction.commit().await?;
    Ok(deleted)
}
