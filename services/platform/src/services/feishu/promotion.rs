use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    services::{product_facts::project_product_facts, product_import::encrypt_confidential},
    state::AppState,
};

use super::{
    activate_product_ownership,
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
    let public_checksum = effective_public_checksum(&record.source_checksum, assets);
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
        let checksum_matches = row
            .try_get::<Value, _>("payload")
            .ok()
            .and_then(|payload| {
                payload
                    .get("sourceChecksum")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .is_some_and(|checksum| checksum == public_checksum);
        let published = row
            .try_get::<Option<i64>, _>("published_revision")
            .ok()
            .flatten()
            .is_some();
        checksum_matches && (published != record.archived)
    }) {
        let product_id: Uuid = existing
            .as_ref()
            .expect("checksum match requires an existing product")
            .try_get("id")?;
        sqlx::query(
            "UPDATE products SET product_import_run_id=$2 WHERE stable_id=$1 AND data_origin='feishu'",
        )
        .bind(stable_id)
        .bind(import_run_id)
        .execute(&mut *transaction)
        .await?;
        activate_product_ownership(
            &mut transaction,
            product_id,
            connector_id,
            &record.wiki_token,
            &record.table_id,
            &record.record_id,
        )
        .await?;
        mark_private_promoted(&mut transaction, import_run_id, stable_id).await?;
        sqlx::query(
            r#"INSERT INTO audit_log
               (id,actor,action,entity_type,entity_id,after_value,reason,request_id,occurred_at)
               VALUES ($1,'feishuSync','feishu.private.refresh','product',$2,$3,
                       'Refresh encrypted private source staging without a public revision',$4,now())"#,
        )
        .bind(Uuid::new_v4())
        .bind(product_id)
        .bind(json!({"privateChecksum": record.confidential_checksum,
                     "mappingVersion": mapping_version}))
        .bind(sync_run_id)
        .execute(&mut *transaction)
        .await?;
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
    payload.insert("sourceChecksum".into(), json!(public_checksum));
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
    let initialize_presentation = before_revision.is_none();
    let presentation = match (existing_presentation, initialize_presentation) {
        (Some(presentation), _) => presentation,
        (None, true) => initial_presentation(&payload)?,
        (None, false) => {
            return Err(ApiError::conflict(
                "An existing product requires a published CMS presentation before Feishu facts can be published.",
            ));
        }
    };
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
    activate_product_ownership(
        &mut transaction,
        product_id,
        connector_id,
        &record.wiki_token,
        &record.table_id,
        &record.record_id,
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

fn effective_public_checksum(fact_checksum: &str, assets: &[StoredSourceAsset]) -> String {
    if assets.is_empty() {
        return fact_checksum.to_owned();
    }
    let mut identities = assets
        .iter()
        .map(|asset| {
            (
                asset.source_field_id.as_str(),
                asset.usage.as_str(),
                asset.checksum.as_str(),
            )
        })
        .collect::<Vec<_>>();
    identities.sort_unstable();
    let payload = serde_json::to_vec(&(fact_checksum, identities))
        .expect("Feishu public checksum input is serializable");
    format!("{:x}", Sha256::digest(payload))
}

#[cfg(test)]
mod checksum_tests {
    use super::*;

    fn asset(checksum: &str) -> StoredSourceAsset {
        StoredSourceAsset {
            media_asset_id: Uuid::nil(),
            storage_key: "key".into(),
            preview_storage_key: None,
            checksum: checksum.into(),
            media_type: "application/pdf".into(),
            byte_size: 1,
            original_name: "curve.pdf".into(),
            source_field_id: "fld-curve".into(),
            source_field_name: "PQ curve".into(),
            usage: "curve".into(),
            newly_created: false,
        }
    }

    #[test]
    fn attachment_content_participates_in_the_public_revision_checksum() {
        let first = effective_public_checksum("facts", &[asset("a")]);
        let second = effective_public_checksum("facts", &[asset("b")]);
        assert_ne!(first, second);
        assert_eq!(
            effective_public_checksum("facts", &[]),
            "facts",
            "no-attachment records retain their normalized fact checksum"
        );
    }
}
