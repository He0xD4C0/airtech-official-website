use super::super::{
    NormalizedFeishuRecord, StoredSourceAsset, VersionedFeishuMapping, CONNECTOR_ID,
};
use super::{Archive, ArchiveObject};
use crate::{error::ApiError, state::AppState};
use airtek_domain::models::FeishuSource;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

pub(super) async fn enabled_sources(state: &AppState) -> Result<Vec<FeishuSource>, ApiError> {
    let value: serde_json::Value =
        sqlx::query_scalar("SELECT sources FROM feishu_connector_settings WHERE connector_id=$1")
            .bind(CONNECTOR_ID)
            .fetch_one(&state.pool)
            .await?;
    serde_json::from_value(value)
        .map_err(|_| ApiError::conflict("Invalid saved source configuration."))
}
pub(super) async fn require_existing_product(
    state: &AppState,
    record: &NormalizedFeishuRecord,
) -> Result<(), ApiError> {
    let matches: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE stable_id=$1 AND data_origin='feishu' AND model=$2 AND status='published' AND payload->>'sourceRecordId'=$3)")
        .bind(&record.source_record_id).bind(record.normalized_payload["model"].as_str())
        .bind(&record.record_id).fetch_one(&state.pool).await?;
    if !matches {
        return Err(ApiError::conflict(
            "Archived identity does not match an existing published product.",
        ));
    }
    Ok(())
}

pub(super) async fn import_metadata(
    state: &AppState,
    archive: &Archive,
    digest: &str,
) -> Result<(u64, u64), ApiError> {
    let mut transaction = state.pool.begin().await?;
    let mut imported = 0_u64;
    let mut unchanged = 0_u64;
    for source in &archive.metadata_sources {
        if source.name.trim().is_empty()
            || source.wiki_token.trim().is_empty()
            || source.table_id.trim().is_empty()
        {
            return Err(ApiError::bad_request(
                "Metadata sources require a name, wiki token, and table id.",
            ));
        }
        let primary_id = source
            .fields
            .iter()
            .find(|field| field.is_primary)
            .map(|field| field.field_id.as_str());
        for record in &source.records {
            let label = primary_id
                .and_then(|field_id| {
                    source
                        .fields
                        .iter()
                        .find(|field| field.field_id == field_id)
                        .and_then(|field| record.fields.get(&field.field_name))
                })
                .and_then(metadata_text)
                .ok_or_else(|| {
                    ApiError::conflict(format!(
                        "{} source record `{}` has no non-empty primary label.",
                        source.name, record.record_id
                    ))
                })?;
            let raw_fields = Value::Object(record.fields.clone());
            let attributes = source
                .fields
                .iter()
                .filter(|field| Some(field.field_id.as_str()) != primary_id)
                .filter_map(|field| {
                    record
                        .fields
                        .get(&field.field_name)
                        .map(|value| (field.field_name.clone(), value.clone()))
                })
                .collect::<Map<_, _>>();
            let record_checksum = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&raw_fields).map_err(|_| {
                    ApiError::internal("Metadata checksum serialization failed.")
                })?)
            );
            let updated = sqlx::query_scalar::<_, Uuid>(
                r#"INSERT INTO product_source_metadata
                   (id,kind,label,source_table,source_record_id,archive_sha256,
                    record_checksum,attributes,raw_fields,captured_at,imported_at)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now())
                   ON CONFLICT (kind,archive_sha256,source_table,source_record_id)
                   DO UPDATE SET
                     label=EXCLUDED.label,
                     record_checksum=EXCLUDED.record_checksum,
                     attributes=EXCLUDED.attributes,
                     raw_fields=EXCLUDED.raw_fields,
                     captured_at=EXCLUDED.captured_at,
                     imported_at=now()
                   WHERE product_source_metadata.record_checksum IS DISTINCT FROM EXCLUDED.record_checksum
                      OR product_source_metadata.label IS DISTINCT FROM EXCLUDED.label
                   RETURNING id"#,
            )
            .bind(Uuid::new_v4())
            .bind(source.kind.as_str())
            .bind(&label)
            .bind(&source.table_id)
            .bind(&record.record_id)
            .bind(digest)
            .bind(&record_checksum)
            .bind(Value::Object(attributes))
            .bind(raw_fields)
            .bind(archive.captured_at)
            .fetch_optional(&mut *transaction)
            .await?;
            if updated.is_some() {
                imported += 1;
            } else {
                unchanged += 1;
            }
        }
    }
    transaction.commit().await?;
    Ok((imported, unchanged))
}
pub(super) async fn existing_assets(
    state: &AppState,
    record: &NormalizedFeishuRecord,
    objects: &HashMap<&str, &ArchiveObject>,
) -> Result<Vec<StoredSourceAsset>, ApiError> {
    let mut output = Vec::new();
    for attachment in &record.attachments {
        let object = objects
            .get(attachment.file_token.as_str())
            .ok_or_else(|| ApiError::conflict("Attachment lacks a verified archived object."))?;
        if object.sha256.len() != 64 || !object.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ApiError::bad_request("Invalid archived object checksum."));
        }
        let row = sqlx::query("SELECT id,storage_key,preview_storage_key,checksum,media_type,byte_size FROM media_assets WHERE checksum=$1 AND byte_size=$2 AND scan_status='clean' AND access_level='public' AND deleted_at IS NULL ORDER BY created_at,id LIMIT 1")
            .bind(&object.sha256).bind(object.byte_size).fetch_optional(&state.pool).await?
            .ok_or_else(|| ApiError::conflict("Archived attachment has no matching validated website object."))?;
        output.push(StoredSourceAsset {
            media_asset_id: row.try_get("id")?,
            storage_key: row.try_get("storage_key")?,
            preview_storage_key: row.try_get("preview_storage_key")?,
            checksum: row.try_get("checksum")?,
            media_type: row.try_get("media_type")?,
            byte_size: row.try_get("byte_size")?,
            original_name: attachment.original_name.clone(),
            source_field_id: attachment.source_field_id.clone(),
            source_field_name: attachment.source_field_name.clone(),
            usage: attachment.usage.clone(),
            newly_created: false,
        });
    }
    Ok(output)
}

pub(super) async fn archive_assets(
    state: &AppState,
    connector_id: Uuid,
    run_id: Uuid,
    record: &NormalizedFeishuRecord,
    objects: &HashMap<&str, &ArchiveObject>,
    object_root: &Path,
) -> Result<Vec<StoredSourceAsset>, ApiError> {
    let mut output = Vec::new();
    for attachment in &record.attachments {
        let object = objects
            .get(attachment.file_token.as_str())
            .ok_or_else(|| ApiError::conflict("Attachment lacks a verified archived object."))?;
        if let Some(asset) = existing_by_checksum(state, object).await? {
            output.push(StoredSourceAsset {
                media_asset_id: asset.0,
                storage_key: asset.1,
                preview_storage_key: asset.2,
                checksum: asset.3,
                media_type: asset.4,
                byte_size: asset.5,
                original_name: attachment.original_name.clone(),
                source_field_id: attachment.source_field_id.clone(),
                source_field_name: attachment.source_field_name.clone(),
                usage: attachment.usage.clone(),
                newly_created: false,
            });
            continue;
        }
        let path = object_path(object_root, object)?;
        output.push(
            super::super::store_archive_asset(
                state,
                connector_id,
                run_id,
                attachment,
                &path,
                &object.sha256,
                object.byte_size,
            )
            .await?,
        );
    }
    Ok(output)
}

pub(super) async fn verify_objects(
    root: &Path,
    objects: &[ArchiveObject],
) -> Result<u64, ApiError> {
    let mut byte_count = 0_u64;
    for object in objects {
        let path = object_path(root, object)?;
        let mut file = tokio::fs::File::open(&path).await.map_err(|error| {
            ApiError::conflict(format!(
                "Archived object `{}` cannot be opened: {error}",
                object.file_token
            ))
        })?;
        let metadata = file.metadata().await.map_err(|error| {
            ApiError::conflict(format!(
                "Archived object metadata could not be read: {error}"
            ))
        })?;
        if metadata.len() != object.byte_size as u64 {
            return Err(ApiError::conflict(format!(
                "Archived object `{}` size does not match its manifest.",
                object.file_token
            )));
        }
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).await.map_err(|error| {
                ApiError::conflict(format!("Archived object could not be read: {error}"))
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let actual = format!("{:x}", hasher.finalize());
        if actual != object.sha256 {
            return Err(ApiError::conflict(format!(
                "Archived object `{}` checksum does not match its manifest.",
                object.file_token
            )));
        }
        byte_count = byte_count.saturating_add(metadata.len());
    }
    Ok(byte_count)
}

fn object_path(root: &Path, object: &ArchiveObject) -> Result<PathBuf, ApiError> {
    let Some(key) = object.object_key.as_deref() else {
        return Ok(root
            .join("objects")
            .join(&object.sha256[..2])
            .join(&object.sha256));
    };
    let relative = Path::new(key);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(ApiError::conflict(
            "Archived object keys must stay within the configured object root.",
        ));
    }
    Ok(root.join(relative))
}

async fn existing_by_checksum(
    state: &AppState,
    object: &ArchiveObject,
) -> Result<Option<(Uuid, String, Option<String>, String, String, i64)>, ApiError> {
    sqlx::query("SELECT id,storage_key,preview_storage_key,checksum,media_type,byte_size FROM media_assets WHERE checksum=$1 AND byte_size=$2 AND scan_status='clean' AND access_level='public' AND deleted_at IS NULL ORDER BY created_at,id LIMIT 1")
        .bind(&object.sha256)
        .bind(object.byte_size)
        .fetch_optional(&state.pool)
        .await?
        .map(|row| Ok((row.try_get("id")?, row.try_get("storage_key")?, row.try_get("preview_storage_key")?, row.try_get("checksum")?, row.try_get("media_type")?, row.try_get("byte_size")?)))
        .transpose()
}

fn metadata_text(value: &Value) -> Option<String> {
    let text = match value {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Array(values) => values
            .iter()
            .filter_map(metadata_text)
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(object) => ["text", "name", "value"]
            .iter()
            .find_map(|key| object.get(*key).and_then(metadata_text))?,
        Value::Null => return None,
    };
    (!text.is_empty()).then_some(text)
}

pub(super) async fn begin_run(
    state: &AppState,
    archive: &Archive,
    mapping: &VersionedFeishuMapping,
    digest: &str,
    mode: &str,
) -> Result<Uuid, ApiError> {
    let id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("feishu:queue:{CONNECTOR_ID}"))
        .execute(&mut *tx)
        .await?;
    let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sync_runs WHERE connector_id=$1 AND status IN ('queued','fetching','validating','readyToPublish'))")
        .bind(CONNECTOR_ID).fetch_one(&mut *tx).await?;
    if active {
        return Err(ApiError::conflict(
            "A Feishu sync is active; archive refresh cannot overlap it.",
        ));
    }
    let revision: i64 = sqlx::query_scalar(
        "SELECT revision FROM feishu_connector_settings WHERE connector_id=$1 FOR UPDATE",
    )
    .bind(CONNECTOR_ID)
    .fetch_one(&mut *tx)
    .await?;
    let (action, reason, mode_label) = match mode {
        "import" => (
            "product.archive.import",
            "Import products, technical assets, and source metadata from the owner-selected local archive",
            "local-archive-import",
        ),
        _ => (
            "product.archive.refresh",
            "Refresh existing products from the owner-selected local archive",
            "local-archive-refresh",
        ),
    };
    let sources = json!(archive
        .sources
        .iter()
        .map(|source| &source.source)
        .collect::<Vec<_>>());
    // Archive mapping is immutable evidence, not a silent replacement of the live connector.
    sqlx::query("INSERT INTO sync_mappings(id,connector_id,version,mapping,schema_version,active) VALUES($1,$2,$3,$4,1,false) ON CONFLICT (connector_id,version) DO NOTHING")
        .bind(Uuid::new_v4()).bind(CONNECTOR_ID).bind(&mapping.version).bind(json!(mapping)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO sync_runs(id,connector_id,source,dry_run,trigger,settings_revision,source_config,mapping_version,status,started_at,payload) VALUES($1,$2,'feishu',false,'manual',$3,$4,$5,'validating',now(),$6)")
        .bind(id).bind(CONNECTOR_ID).bind(revision).bind(sources).bind(&mapping.version)
        .bind(json!({"archiveSha256":digest,"capturedAt":archive.captured_at,"mode":mode_label})).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO product_import_runs(id,sync_run_id,connector_id,environment,data_origin,dry_run,status,mapping_version,source_checksum,records_received,records_valid,error_count,started_at,created_at) VALUES($1,$1,$2,$3,'feishu',false,'processing',$4,$5,0,0,0,now(),now())")
        .bind(id).bind(CONNECTOR_ID).bind(state.environment_label()).bind(&mapping.version).bind(digest).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO audit_log(id,actor,action,entity_type,entity_id,after_value,reason,request_id,occurred_at) VALUES($1,'maintenance',$5,'sourceConnector',$2,$3,$6,$4,now())")
        .bind(Uuid::new_v4()).bind(CONNECTOR_ID).bind(json!({"archiveSha256":digest,"mappingVersion":mapping.version})).bind(id)
        .bind(action).bind(reason).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(id)
}
pub(super) async fn fail_run(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    sqlx::query("UPDATE sync_runs SET status='failed',completed_at=now(),error='Archive refresh stopped; completed product revisions remain auditable' WHERE id=$1").bind(id).execute(&state.pool).await?;
    sqlx::query("UPDATE product_import_runs SET status='failed',completed_at=now() WHERE id=$1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::object_path;
    use super::ArchiveObject;
    use std::path::Path;

    fn object(key: Option<&str>) -> ArchiveObject {
        ArchiveObject {
            file_token: "token".into(),
            sha256: "aa".repeat(32),
            byte_size: 1,
            object_key: key.map(str::to_owned),
        }
    }

    #[test]
    fn archived_object_keys_cannot_escape_the_object_root() {
        assert!(object_path(Path::new("/archive"), &object(Some("../secret"))).is_err());
        assert!(object_path(Path::new("/archive"), &object(Some("/secret"))).is_err());
        assert!(object_path(Path::new("/archive"), &object(Some("objects/aa/content"))).is_ok());
    }
}
