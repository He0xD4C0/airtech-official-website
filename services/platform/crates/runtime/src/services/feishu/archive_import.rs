use super::storage;
use super::{
    discover_mapping, normalize_record, set_source_warnings, source_identity_key, Archive,
    DiscoveredSource, NormalizedFeishuRecord, PromotionOutcome,
};
use crate::{error::ApiError, state::AppState};
use airtek_domain::models::ValidationIssue;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

#[derive(Default)]
struct SourceImportStats {
    records_received: usize,
    valid: usize,
    invalid: usize,
}

pub async fn import_local_archive(
    state: &AppState,
    bytes: &[u8],
    apply_checksum: Option<&str>,
    object_root: &Path,
) -> Result<Value, ApiError> {
    if bytes.len() > 25 * 1024 * 1024 {
        return Err(ApiError::bad_request("Archive exceeds 25 MiB."));
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    if apply_checksum.is_some_and(|expected| expected != digest) {
        return Err(ApiError::conflict(
            "Archive checksum does not match the reviewed plan.",
        ));
    }
    let archive: Archive = serde_json::from_slice(bytes)
        .map_err(|_| ApiError::bad_request("Invalid frozen archive schema."))?;
    if archive.schema_version != 1 || archive.sources.is_empty() {
        return Err(ApiError::bad_request(
            "Archive schemaVersion must be 1 with selected sources.",
        ));
    }
    let discovered = archive
        .sources
        .iter()
        .map(|source| DiscoveredSource {
            source: source.source.clone(),
            app_token: String::new(),
            fields: source.fields.clone(),
        })
        .collect::<Vec<_>>();
    let mapping_version = format!("feishu-archive-v3-{}", &digest[..16]);
    let mapping = discover_mapping(&mapping_version, &discovered)?;
    let objects: HashMap<_, _> = archive
        .objects
        .iter()
        .map(|object| (object.file_token.as_str(), object))
        .collect();
    if objects.len() != archive.objects.len() {
        return Err(ApiError::conflict("Duplicate archived object tokens."));
    }
    let mut object_bytes = 0_i64;
    for object in &archive.objects {
        if object.sha256.len() != 64
            || !object
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || object.byte_size < 0
        {
            return Err(ApiError::bad_request(
                "Archive object manifest contains an invalid checksum or size.",
            ));
        }
        object_bytes = object_bytes.saturating_add(object.byte_size);
    }
    let verified_object_bytes = storage::verify_objects(object_root, &archive.objects).await?;
    debug_assert_eq!(verified_object_bytes, object_bytes as u64);
    let mut valid = Vec::<NormalizedFeishuRecord>::new();
    let mut invalid = Vec::<NormalizedFeishuRecord>::new();
    let mut models = BTreeMap::<String, usize>::new();
    let mut warnings = BTreeMap::<String, usize>::new();
    let mut source_stats = BTreeMap::<String, SourceImportStats>::new();
    let mut metadata_stats = BTreeMap::<String, usize>::new();
    let mut identities = std::collections::BTreeSet::new();
    for source in &archive.sources {
        let stats = source_stats
            .entry(source.source.table_id.clone())
            .or_default();
        let table_mapping = &mapping.tables[&source_identity_key(&source.source)];
        for record in &source.records {
            stats.records_received += 1;
            let mut normalized = normalize_record(&source.source, table_mapping, record);
            if !identities.insert(normalized.source_record_id.clone()) {
                return Err(ApiError::conflict(
                    "Duplicate source record identity in archive.",
                ));
            }
            normalized.snapshot_payload["archive"] = json!({
                "sha256": digest,
                "capturedAt": archive.captured_at,
            });
            for warning in &normalized.warnings {
                *warnings.entry(warning.code.clone()).or_default() += 1;
            }
            if normalized.issues.is_empty() && !normalized.archived {
                let model = normalized.normalized_payload["model"]
                    .as_str()
                    .unwrap_or_default();
                *models.entry(model.to_owned()).or_default() += 1;
                stats.valid += 1;
                valid.push(normalized);
            } else {
                stats.invalid += 1;
                invalid.push(normalized);
            }
        }
    }
    for source in &archive.metadata_sources {
        let mut metadata_ids = std::collections::BTreeSet::new();
        for record in &source.records {
            if !metadata_ids.insert(record.record_id.as_str()) {
                return Err(ApiError::conflict(
                    "Duplicate metadata source record identity in archive.",
                ));
            }
        }
        *metadata_stats
            .entry(source.kind.as_str().into())
            .or_default() += source.records.len();
    }
    let duplicate_models = models
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(model, count)| json!({"model": model, "records": count}))
        .collect::<Vec<_>>();
    for record in &mut valid {
        let model = record.normalized_payload["model"]
            .as_str()
            .unwrap_or_default();
        let Some(count) = models.get(model).copied().filter(|count| *count > 1) else {
            continue;
        };
        let detail = format!(
            "Model `{model}` appears in {count} source records; duplicate models are retained separately and are not merged."
        );
        record.warnings.push(ValidationIssue {
            field_path: "model".into(),
            code: "duplicateModel".into(),
            detail: detail.clone(),
        });
        set_source_warnings(
            record,
            vec![json!({"code": "duplicateModel", "detail": detail})],
        );
    }
    let source_stats = source_stats
        .into_iter()
        .map(|(table_id, stats)| {
            (
                table_id,
                json!({
                    "recordsReceived": stats.records_received,
                    "valid": stats.valid,
                    "invalid": stats.invalid,
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let mut report = json!({
        "archiveSha256": digest,
        "capturedAt": archive.captured_at,
        "mappingVersion": mapping_version,
        "validRecords": valid.len(),
        "invalidRecords": invalid.len(),
        "objectCount": archive.objects.len(),
        "objectBytes": verified_object_bytes,
        "sourceStats": source_stats,
        "metadataSourceRecords": metadata_stats,
        "duplicateModels": duplicate_models,
        "warnings": warnings,
        "applied": false,
    });
    let Some(_) = apply_checksum else {
        return Ok(report);
    };
    let run_id = storage::begin_run(state, &archive, &mapping, &digest, "import").await?;
    let result = async {
        let mut assets_copied = 0u64;
        let mut assets_reused = 0u64;
        let mut created = 0u64;
        for (index, record) in valid.iter().enumerate() {
            let assets = storage::archive_assets(
                state,
                super::super::CONNECTOR_ID,
                run_id,
                record,
                &objects,
                object_root,
            )
            .await?;
            assets_copied += assets.iter().filter(|asset| asset.newly_created).count() as u64;
            assets_reused += assets.iter().filter(|asset| !asset.newly_created).count() as u64;
            if matches!(
                super::super::promote_record(
                    state,
                    super::super::CONNECTOR_ID,
                    run_id,
                    run_id,
                    &mapping_version,
                    &digest,
                    index as i32 + 2,
                    record,
                    &assets,
                )
                .await?,
                PromotionOutcome::Created
            ) {
                created += 1;
            }
        }
        for (index, record) in invalid.iter().enumerate() {
            super::super::stage_invalid_record(
                state,
                super::super::CONNECTOR_ID,
                run_id,
                run_id,
                &mapping_version,
                &digest,
                (valid.len() + index) as i32 + 2,
                record,
            )
            .await?;
        }
        let (metadata_imported, metadata_unchanged) =
            storage::import_metadata(state, &archive, &digest).await?;
        super::super::runner_support::finalize_run(state, run_id, assets_copied, assets_reused, 0)
            .await?;
        report["applied"] = json!(true);
        report["runId"] = json!(run_id);
        report["createdProducts"] = json!(created);
        report["assetsCopied"] = json!(assets_copied);
        report["assetsReused"] = json!(assets_reused);
        report["metadataImported"] = json!(metadata_imported);
        report["metadataUnchanged"] = json!(metadata_unchanged);
        Ok::<(), ApiError>(())
    }
    .await;
    if let Err(error) = result {
        storage::fail_run(state, run_id).await?;
        return Err(error);
    }
    Ok(report)
}
