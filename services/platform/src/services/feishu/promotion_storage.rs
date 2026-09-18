use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::error::ApiError;

use super::{NormalizedFeishuRecord, StoredSourceAsset};

pub(super) struct Presentation {
    pub(super) slug: String,
    pub(super) title: String,
    pub(super) summary: Option<String>,
    pub(super) content: Value,
    pub(super) seo: Value,
    pub(super) is_placeholder: bool,
    pub(super) indexable: bool,
    pub(super) data_origin: String,
    pub(super) updated_by: String,
}

pub(super) async fn load_presentation(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
) -> Result<Option<Presentation>, ApiError> {
    sqlx::query(
        r#"SELECT slug,title,summary,content,seo_metadata,
                  is_placeholder,indexable,data_origin,updated_by
           FROM product_presentation_working
           WHERE product_id=$1 AND locale='en' FOR UPDATE"#,
    )
    .bind(product_id)
    .fetch_optional(&mut **transaction)
    .await?
    .map(|row| {
        Ok(Presentation {
            slug: row.try_get("slug")?,
            title: row.try_get("title")?,
            summary: row.try_get("summary")?,
            content: row.try_get("content")?,
            seo: row.try_get("seo_metadata")?,
            is_placeholder: row.try_get("is_placeholder")?,
            indexable: row.try_get("indexable")?,
            data_origin: row.try_get("data_origin")?,
            updated_by: row.try_get("updated_by")?,
        })
    })
    .transpose()
}

pub(super) fn initial_presentation(payload: &Value) -> Result<Presentation, ApiError> {
    let slug = required_text(payload, "slug")?.to_owned();
    let title = required_text(payload, "title")?.to_owned();
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let content = json!({"sortOrder": 0, "relatedContentIds": [], "mediaGallery": []});
    let seo = payload.get("seo").cloned().unwrap_or_else(|| {
        json!({
            "title": null, "description": null, "canonicalPath": null, "indexable": true
        })
    });
    Ok(Presentation {
        slug,
        title,
        summary,
        content,
        seo,
        is_placeholder: false,
        indexable: true,
        data_origin: "feishu".into(),
        updated_by: "feishuSync".into(),
    })
}

pub(super) async fn persist_initial_presentation(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    source_revision: i64,
    presentation: &Presentation,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO product_presentation_working
           (product_id,locale,current_revision,published_revision,slug,title,summary,
            content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
            updated_by,updated_at)
           VALUES ($1,'en',1,1,$2,$3,$4,$5,$6,'verified',false,true,'feishu',
                   'feishuSync',$7)"#,
    )
    .bind(product_id)
    .bind(&presentation.slug)
    .bind(&presentation.title)
    .bind(&presentation.summary)
    .bind(&presentation.content)
    .bind(&presentation.seo)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO product_presentation_revisions
           (product_id,locale,revision,source_product_revision,slug,title,summary,
            content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
            created_by,created_at)
           VALUES ($1,'en',1,$2,$3,$4,$5,$6,$7,'verified',false,true,'feishu',
                   'feishuSync',$8)"#,
    )
    .bind(product_id)
    .bind(source_revision)
    .bind(&presentation.slug)
    .bind(&presentation.title)
    .bind(&presentation.summary)
    .bind(&presentation.content)
    .bind(&presentation.seo)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn insert_product_shell(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    snapshot_id: Uuid,
    import_run_id: Uuid,
    payload: &Value,
    record: &NormalizedFeishuRecord,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), ApiError> {
    let model = payload.get("model").and_then(Value::as_str);
    let family = required_text(payload, "family")?;
    let stable_id = required_text(payload, "stableId")?;
    let slug = required_text(payload, "slug")?;
    let status = if record.archived {
        "archived"
    } else {
        "published"
    };
    let published = (!record.archived).then_some(1_i64);
    sqlx::query(
        r#"INSERT INTO products
           (id,stable_id,model,slug,locale,family,source_snapshot_id,source_revision,
            status,current_revision,published_revision,indexable,payload,updated_at,
            data_origin,product_import_run_id)
           VALUES ($1,$2,$3,$4,'en',$5,$6,$7,$8,1,$9,$10,$11,$12,'feishu',$13)"#,
    )
    .bind(product_id)
    .bind(stable_id)
    .bind(model)
    .bind(slug)
    .bind(family)
    .bind(snapshot_id)
    .bind(&record.source_revision)
    .bind(status)
    .bind(published)
    .bind(!record.archived)
    .bind(payload)
    .bind(now)
    .bind(import_run_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn persist_product_revision(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    revision: i64,
    snapshot_id: Uuid,
    import_run_id: Uuid,
    payload: &Value,
    record: &NormalizedFeishuRecord,
    presentation: &Presentation,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), ApiError> {
    let model = payload.get("model").and_then(Value::as_str);
    let family = required_text(payload, "family")?;
    let status = if record.archived {
        "archived"
    } else {
        "published"
    };
    let published = (!record.archived).then_some(revision);
    sqlx::query(
        r#"UPDATE products SET model=$2,slug=$3,locale='en',family=$4,
              source_snapshot_id=$5,source_revision=$6,status=$7,current_revision=$8,
              published_revision=$9,indexable=$10,payload=$11,updated_at=$12,
              data_origin='feishu',product_import_run_id=$13 WHERE id=$1"#,
    )
    .bind(product_id)
    .bind(model)
    .bind(&presentation.slug)
    .bind(family)
    .bind(snapshot_id)
    .bind(&record.source_revision)
    .bind(status)
    .bind(revision)
    .bind(published)
    .bind(presentation.indexable && !record.archived)
    .bind(payload)
    .bind(now)
    .bind(import_run_id)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO product_revisions
           (product_id,revision,source_snapshot_id,payload,created_at,data_origin,product_import_run_id)
           VALUES ($1,$2,$3,$4,$5,'feishu',$6)"#,
    )
    .bind(product_id)
    .bind(revision)
    .bind(snapshot_id)
    .bind(payload)
    .bind(now)
    .bind(import_run_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(super) async fn persist_asset_references(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    revision: i64,
    source_record_id: &str,
    assets: &[StoredSourceAsset],
) -> Result<(), ApiError> {
    for (index, asset) in assets.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO asset_references
               (id,media_asset_id,product_id,product_revision,usage,sort_order,
                source_field_id,source_field_name,source_record_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(Uuid::new_v4())
        .bind(asset.media_asset_id)
        .bind(product_id)
        .bind(revision)
        .bind(&asset.usage)
        .bind(index as i32)
        .bind(&asset.source_field_id)
        .bind(&asset.source_field_name)
        .bind(source_record_id)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

pub(super) async fn publish_localization_and_route(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    revision: i64,
    presentation: &Presentation,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO product_localizations
           (product_id,product_revision,locale,slug,title,summary,content,seo_metadata,
            translation_state,is_placeholder,indexable,data_origin,updated_by,updated_at)
           VALUES ($1,$2,'en',$3,$4,$5,$6,$7,'verified',$8,$9,$10,$11,$12)"#,
    )
    .bind(product_id)
    .bind(revision)
    .bind(&presentation.slug)
    .bind(&presentation.title)
    .bind(&presentation.summary)
    .bind(&presentation.content)
    .bind(&presentation.seo)
    .bind(presentation.is_placeholder)
    .bind(presentation.indexable && !presentation.is_placeholder)
    .bind(&presentation.data_origin)
    .bind(&presentation.updated_by)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"UPDATE product_presentation_working SET published_revision=current_revision,
                  translation_state='verified' WHERE product_id=$1 AND locale='en'"#,
    )
    .bind(product_id)
    .execute(&mut **transaction)
    .await?;
    sqlx::query("DELETE FROM public_routes WHERE entity_type='product' AND entity_id=$1")
        .bind(product_id)
        .execute(&mut **transaction)
        .await?;
    let path = presentation
        .seo
        .get("canonicalPath")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ApiError::conflict("An automatically published product requires a canonical path.")
        })?;
    sqlx::query(
        r#"INSERT INTO public_routes
           (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
           VALUES ($1,'product',$2,'en',$3,$4,$5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(product_id)
    .bind(path)
    .bind(presentation.indexable && !presentation.is_placeholder)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn persist_staging(
    transaction: &mut Transaction<'_, Postgres>,
    connector_id: Uuid,
    sync_run_id: Uuid,
    import_run_id: Uuid,
    source_row_number: i32,
    record: &NormalizedFeishuRecord,
    encrypted: &[u8],
    validation_status: &str,
) -> Result<Uuid, ApiError> {
    if encrypted.len() < 28 {
        return Err(ApiError::internal("Encrypted Feishu staging is invalid."));
    }
    let snapshot_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO source_snapshots
           (id,connector_id,sync_run_id,source_record_id,source_revision,checksum,source_payload)
           VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
    )
    .bind(snapshot_id)
    .bind(connector_id)
    .bind(sync_run_id)
    .bind(&record.source_record_id)
    .bind(&record.source_revision)
    .bind(&record.source_checksum)
    .bind(&record.snapshot_payload)
    .execute(&mut **transaction)
    .await?;
    let staging_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO staging_records
           (id,sync_run_id,source_snapshot_id,source_record_id,validation_status,
            normalized_payload,validation_errors)
           VALUES ($1,$2,$3,$4,$5,$6,$7)"#,
    )
    .bind(staging_id)
    .bind(sync_run_id)
    .bind(snapshot_id)
    .bind(&record.source_record_id)
    .bind(validation_status)
    .bind(&record.normalized_payload)
    .bind(json!(record.issues))
    .execute(&mut **transaction)
    .await?;
    let nonce = &encrypted[..12];
    let tag = &encrypted[encrypted.len() - 16..];
    let ciphertext = &encrypted[12..encrypted.len() - 16];
    sqlx::query(
        r#"INSERT INTO product_import_private_staging
           (id,import_run_id,source_record_id,source_row_number,ciphertext,encryption_algorithm,
            encryption_key_id,nonce,authentication_tag,checksum,status,promoted_staging_record_id,
            created_at,expires_at)
           VALUES ($1,$2,$3,$4,$5,'AES-256-GCM','environment-v1',$6,$7,$8,
                   'validated',$9,now(),now()+interval '30 days')"#,
    )
    .bind(Uuid::new_v4())
    .bind(import_run_id)
    .bind(&record.source_record_id)
    .bind(source_row_number)
    .bind(ciphertext)
    .bind(nonce)
    .bind(tag)
    .bind(format!("{:x}", Sha256::digest(encrypted)))
    .bind(staging_id)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO product_import_normalized_records
           (id,import_run_id,source_record_id,normalized_payload,validation_status)
           VALUES ($1,$2,$3,$4,$5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(import_run_id)
    .bind(&record.source_record_id)
    .bind(&record.normalized_payload)
    .bind(validation_status)
    .execute(&mut **transaction)
    .await?;
    Ok(snapshot_id)
}

pub(super) async fn mark_private_promoted(
    transaction: &mut Transaction<'_, Postgres>,
    import_run_id: Uuid,
    stable_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"UPDATE product_import_private_staging SET status='promoted',processed_at=now()
           WHERE import_run_id=$1 AND source_record_id=$2"#,
    )
    .bind(import_run_id)
    .bind(stable_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(super) fn required_text<'a>(value: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::conflict(format!("Normalized Feishu `{key}` is required.")))
}
