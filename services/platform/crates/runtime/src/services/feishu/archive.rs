//! Refresh existing Product Master revisions from a frozen, checksummed local archive.
//! This command never creates, removes or merges products, and never writes objects.
use super::{
    discover_mapping, normalize_record, set_source_warnings, source_identity_key, DiscoveredSource,
    FeishuField, FeishuRecord, NormalizedFeishuRecord, PromotionOutcome, StoredSourceAsset,
};
use crate::{error::ApiError, state::AppState};
use airtek_domain::models::{FeishuSource, SourceMetadataKind};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

#[path = "archive_storage.rs"]
mod storage;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Archive {
    pub schema_version: u32,
    pub captured_at: chrono::DateTime<chrono::Utc>,
    pub sources: Vec<ArchiveSource>,
    #[serde(default)]
    pub metadata_sources: Vec<ArchiveMetadataSource>,
    pub objects: Vec<ArchiveObject>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchiveSource {
    pub source: FeishuSource,
    pub fields: Vec<FeishuField>,
    pub records: Vec<FeishuRecord>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ArchiveMetadataSource {
    pub kind: SourceMetadataKind,
    pub name: String,
    pub wiki_token: String,
    pub table_id: String,
    pub fields: Vec<FeishuField>,
    pub records: Vec<FeishuRecord>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ArchiveObject {
    pub file_token: String,
    pub sha256: String,
    pub byte_size: i64,
    #[serde(default)]
    pub object_key: Option<String>,
}

#[path = "archive_import.rs"]
mod archive_import;
pub use archive_import::import_local_archive;

pub async fn refresh_existing_archive(
    state: &AppState,
    bytes: &[u8],
    apply_checksum: Option<&str>,
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
    let enabled = storage::enabled_sources(state).await?;
    for source in &archive.sources {
        if !enabled.iter().any(|current| {
            current.enabled
                && source_identity_key(current) == source_identity_key(&source.source)
                && current.family == source.source.family
                && current.application == source.source.application
        }) {
            return Err(ApiError::conflict(
                "Archive source is not an enabled, matching website product source.",
            ));
        }
    }
    let discovered: Vec<_> = archive
        .sources
        .iter()
        .map(|source| DiscoveredSource {
            source: source.source.clone(),
            app_token: String::new(),
            fields: source.fields.clone(),
        })
        .collect();
    let mapping_version = format!("feishu-archive-v2-{}", &digest[..16]);
    let mapping = discover_mapping(&mapping_version, &discovered)?;
    let objects: HashMap<_, _> = archive
        .objects
        .iter()
        .map(|object| (object.file_token.as_str(), object))
        .collect();
    if objects.len() != archive.objects.len() {
        return Err(ApiError::conflict("Duplicate archived object tokens."));
    }
    let mut prepared: Vec<(NormalizedFeishuRecord, Vec<StoredSourceAsset>)> = Vec::new();
    let mut invalid = Vec::new();
    let mut models = BTreeMap::<String, usize>::new();
    let mut warning_counts = BTreeMap::<String, usize>::new();
    let mut identities = std::collections::BTreeSet::new();
    for source in &archive.sources {
        let table_mapping = &mapping.tables[&source_identity_key(&source.source)];
        for record in &source.records {
            let mut normalized = normalize_record(&source.source, table_mapping, record);
            if !identities.insert(normalized.source_record_id.clone()) {
                return Err(ApiError::conflict(
                    "Duplicate source record identity in archive.",
                ));
            }
            normalized.snapshot_payload["archive"] =
                json!({"sha256":digest,"capturedAt":archive.captured_at});
            for warning in &normalized.warnings {
                *warning_counts.entry(warning.code.clone()).or_default() += 1;
            }
            if !normalized.issues.is_empty() {
                invalid.push(normalized);
                continue;
            }
            if normalized.archived {
                return Err(ApiError::conflict(
                    "Archive refresh cannot archive a live product.",
                ));
            }
            storage::require_existing_product(state, &normalized).await?;
            *models
                .entry(
                    normalized.normalized_payload["model"]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                )
                .or_default() += 1;
            let assets = storage::existing_assets(state, &normalized, &objects).await?;
            prepared.push((normalized, assets));
        }
    }
    let duplicates: Vec<_> = models
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(model, count)| json!({"model":model,"records":count}))
        .collect();
    let mut report = json!({"archiveSha256":digest,"capturedAt":archive.captured_at,
        "mappingVersion":mapping_version,"existingProducts":prepared.len(),"invalidRecords":invalid.len(),
        "reusedAttachments":prepared.iter().map(|(_, assets)| assets.len()).sum::<usize>(),
        "duplicateModels":duplicates,"warnings":warning_counts,"applied":false});
    if apply_checksum.is_none() {
        return Ok(report);
    }
    let run_id = storage::begin_run(state, &archive, &mapping, &digest, "refresh").await?;
    let result = async {
        let mut updated = 0;
        let mut unchanged = 0;
        for (index, (record, assets)) in prepared.iter().enumerate() {
            match super::promote_record(
                state,
                super::CONNECTOR_ID,
                run_id,
                run_id,
                &mapping_version,
                &digest,
                index as i32 + 2,
                record,
                assets,
            )
            .await?
            {
                PromotionOutcome::Updated => updated += 1,
                PromotionOutcome::Unchanged => unchanged += 1,
                _ => {
                    return Err(ApiError::conflict(
                        "Existing archive refresh unexpectedly changed product lifecycle.",
                    ))
                }
            }
        }
        for (index, record) in invalid.iter().enumerate() {
            super::stage_invalid_record(
                state,
                super::CONNECTOR_ID,
                run_id,
                run_id,
                &mapping_version,
                &digest,
                (prepared.len() + index) as i32 + 2,
                record,
            )
            .await?;
        }
        super::runner_support::finalize_run(
            state,
            run_id,
            0,
            prepared.iter().map(|(_, assets)| assets.len() as u64).sum(),
            0,
        )
        .await?;
        report["applied"] = json!(true);
        report["runId"] = json!(run_id);
        report["updatedProducts"] = json!(updated);
        report["unchangedProducts"] = json!(unchanged);
        Ok::<(), ApiError>(())
    }
    .await;
    if let Err(error) = result {
        storage::fail_run(state, run_id).await?;
        return Err(error);
    }
    Ok(report)
}
