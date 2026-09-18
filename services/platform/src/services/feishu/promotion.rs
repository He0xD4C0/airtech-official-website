use chrono::Utc;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    services::{product_facts::project_product_facts, product_import::encrypt_confidential},
    state::AppState,
};

use super::{
    promotion_storage::{
        initial_presentation, insert_product_shell, load_presentation, mark_private_promoted,
        persist_asset_references, persist_initial_presentation, persist_product_revision,
        persist_staging, publish_localization_and_route, required_text,
    },
    NormalizedFeishuRecord, StoredSourceAsset,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromotionOutcome {
    Created,
    Updated,
    Unchanged,
    Archived,
}

#[allow(clippy::too_many_arguments)]
pub async fn promote_record(
    state: &AppState,
    connector_id: Uuid,
    sync_run_id: Uuid,
    import_run_id: Uuid,
    mapping_version: &str,
    import_checksum: &str,
    source_row_number: i32,
    record: &NormalizedFeishuRecord,
    assets: &[StoredSourceAsset],
) -> Result<PromotionOutcome, ApiError> {
    let stable_id = required_text(&record.normalized_payload, "stableId")?;
    let plaintext = serde_json::to_vec(&record.confidential_payload)
        .map_err(|_| ApiError::internal("Feishu private staging serialization failed."))?;
    let key = state
        .config
        .product_staging_encryption_key
        .as_ref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Product staging encryption must be configured for Feishu synchronization.",
            )
        })?;
    let aad = format!("{mapping_version}:{import_checksum}:{source_row_number}:{stable_id}");
    let encrypted = encrypt_confidential(key, aad.as_bytes(), &plaintext)?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("feishu:product:{stable_id}"))
        .execute(&mut *transaction)
        .await?;
    let snapshot_id = persist_staging(
        &mut transaction,
        connector_id,
        sync_run_id,
        import_run_id,
        source_row_number,
        record,
        &encrypted,
        "valid",
    )
    .await?;
    let existing = sqlx::query(
        r#"SELECT id,current_revision,published_revision,data_origin,payload
           FROM products WHERE stable_id=$1 FOR UPDATE"#,
    )
    .bind(stable_id)
    .fetch_optional(&mut *transaction)
    .await?;
    if existing.as_ref().is_some_and(|row| {
        row.try_get::<Value, _>("payload")
            .ok()
            .and_then(|payload| {
                payload
                    .get("sourceChecksum")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .is_some_and(|checksum| checksum == record.source_checksum)
    }) {
        sqlx::query(
            "UPDATE products SET product_import_run_id=$2 WHERE stable_id=$1 AND data_origin='feishu'",
        )
        .bind(stable_id)
        .bind(import_run_id)
        .execute(&mut *transaction)
        .await?;
        mark_private_promoted(&mut transaction, import_run_id, stable_id).await?;
        transaction.commit().await?;
        return Ok(PromotionOutcome::Unchanged);
    }
    if existing.as_ref().is_some_and(|row| {
        row.try_get::<String, _>("data_origin").ok().as_deref() == Some("developmentFixture")
    }) {
        return Err(ApiError::conflict(
            "A development fixture cannot be taken over by Feishu.",
        ));
    }

    let now = Utc::now();
    let product_id = existing
        .as_ref()
        .map(|row| row.try_get("id"))
        .transpose()?
        .unwrap_or_else(Uuid::new_v4);
    let before_revision = existing
        .as_ref()
        .map(|row| row.try_get::<i64, _>("current_revision"))
        .transpose()?;
    let revision = before_revision.unwrap_or_default() + 1;
    let mut payload = record
        .normalized_payload
        .as_object()
        .cloned()
        .ok_or_else(|| ApiError::internal("Normalized Feishu payload is not an object."))?;
    let source_assets = assets
        .iter()
        .map(|asset| {
            json!({
                "assetId": asset.media_asset_id,
                "usage": asset.usage,
                "sha256": asset.checksum,
                "mediaType": asset.media_type,
                "byteSize": asset.byte_size
            })
        })
        .collect::<Vec<_>>();
    payload.insert("id".into(), json!(product_id));
    payload.insert("sourceSnapshotId".into(), json!(snapshot_id));
    payload.insert("currentRevision".into(), json!(revision));
    payload.insert(
        "publishedRevision".into(),
        if record.archived {
            Value::Null
        } else {
            json!(revision)
        },
    );
    payload.insert(
        "status".into(),
        json!(if record.archived {
            "archived"
        } else {
            "published"
        }),
    );
    payload.insert("indexable".into(), json!(!record.archived));
    payload.insert("updatedAt".into(), json!(now));
    payload.insert("dataOrigin".into(), json!("feishu"));
    payload.insert("sourceAssets".into(), Value::Array(source_assets));
    let mut payload = Value::Object(payload);
    if before_revision.is_none() {
        insert_product_shell(
            &mut transaction,
            product_id,
            snapshot_id,
            import_run_id,
            &payload,
            record,
            now,
        )
        .await?;
    }
    let existing_presentation = load_presentation(&mut transaction, product_id).await?;
    let initialize_presentation = existing_presentation.is_none();
    let presentation = existing_presentation
        .map(Ok)
        .unwrap_or_else(|| initial_presentation(&payload))?;
    if let Some(object) = payload.as_object_mut() {
        object.insert("slug".into(), json!(presentation.slug));
        object.insert("title".into(), json!(presentation.title));
        object.insert("summary".into(), json!(presentation.summary));
        object.insert("seo".into(), presentation.seo.clone());
        object.insert(
            "indexable".into(),
            json!(presentation.indexable && !record.archived),
        );
    }
    persist_product_revision(
        &mut transaction,
        product_id,
        revision,
        snapshot_id,
        import_run_id,
        &payload,
        record,
        &presentation,
        now,
    )
    .await?;
    if initialize_presentation {
        persist_initial_presentation(&mut transaction, product_id, revision, &presentation, now)
            .await?;
    }
    project_product_facts(
        &mut transaction,
        product_id,
        revision,
        &payload,
        &format!("feishu:{}:{stable_id}", record.source_revision),
    )
    .await?;
    persist_asset_references(
        &mut transaction,
        product_id,
        revision,
        &record.source_record_id,
        assets,
    )
    .await?;
    if record.archived {
        sqlx::query("DELETE FROM public_routes WHERE entity_type='product' AND entity_id=$1")
            .bind(product_id)
            .execute(&mut *transaction)
            .await?;
    } else {
        publish_localization_and_route(&mut transaction, product_id, revision, &presentation, now)
            .await?;
    }
    let kind = if record.archived {
        "archived"
    } else if before_revision.is_none() {
        "created"
    } else {
        "updated"
    };
    sqlx::query(
        r#"INSERT INTO feishu_sync_changes
           (sync_run_id,product_id,before_revision,after_revision,change_kind)
           VALUES ($1,$2,$3,$4,$5)
           ON CONFLICT (sync_run_id,product_id) DO UPDATE SET
             after_revision=EXCLUDED.after_revision,change_kind=EXCLUDED.change_kind"#,
    )
    .bind(sync_run_id)
    .bind(product_id)
    .bind(before_revision)
    .bind(revision)
    .bind(kind)
    .execute(&mut *transaction)
    .await?;
    let topic = if record.archived {
        "public.product.unpublished"
    } else {
        "public.product.published"
    };
    sqlx::query(
        r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
           VALUES ($1,$2,'product',$3,$4)"#,
    )
    .bind(Uuid::new_v4())
    .bind(topic)
    .bind(product_id)
    .bind(json!({"entityId": product_id, "revision": revision, "locale": "en"}))
    .execute(&mut *transaction)
    .await?;
    mark_private_promoted(&mut transaction, import_run_id, stable_id).await?;
    transaction.commit().await?;
    Ok(match kind {
        "created" => PromotionOutcome::Created,
        "archived" => PromotionOutcome::Archived,
        _ => PromotionOutcome::Updated,
    })
}
