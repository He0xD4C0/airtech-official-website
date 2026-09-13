use serde_json::{json, Map};
use uuid::Uuid;

use crate::models::{ContentBlock, MigrationPreflightIssueCode};

use super::{
    conversion::convert_content,
    tests::{content_payload, source},
};

fn embedded(asset_id: Uuid, description: serde_json::Value) -> serde_json::Value {
    json!({
        "downloadAsset": {
            "asset": {
                "assetId": asset_id,
            },
            "label": "File",
            "description": description
        }
    })
}

fn seed(asset_id: Uuid) -> Map<String, serde_json::Value> {
    json!({
        "asset": {
            "assetId": asset_id,
        }
    })
    .as_object()
    .cloned()
    .expect("seed")
}

#[test]
fn reconciles_download_asset_and_description_from_both_legacy_sources() {
    let asset_id = Uuid::from_u128(1_000);
    let payload = content_payload(
        "downloadDetail",
        json!({"fileDescription": "Controlled file"}),
        embedded(asset_id, serde_json::Value::Null),
    );
    let (record, issues) = convert_content(source("download", "file", payload), seed(asset_id));
    let draft = record.expect("consistent download").candidate;
    let description = draft
        .composition
        .blocks
        .iter()
        .find_map(|block| match block {
            ContentBlock::DownloadAsset(block) => block.description.as_deref(),
            _ => None,
        });
    assert_eq!(description, Some("Controlled file"));
    assert!(!issues
        .iter()
        .any(|issue| issue.code == MigrationPreflightIssueCode::MissingMediaAsset));

    let payload = content_payload(
        "downloadDetail",
        json!({"fileDescription": "Controlled file"}),
        embedded(asset_id, json!("Controlled file")),
    );
    let (record, issues) = convert_content(source("download", "file", payload), Map::new());
    assert!(record.is_some(), "embedded block preserves description");
    assert!(!issues
        .iter()
        .any(|issue| issue.code == MigrationPreflightIssueCode::MissingMediaAsset));
}

#[test]
fn blocks_conflicting_download_asset_or_description() {
    let referenced = Uuid::from_u128(1_010);
    let embedded_id = Uuid::from_u128(1_011);
    let payload = content_payload(
        "downloadDetail",
        json!({"fileDescription": "Authoritative"}),
        embedded(embedded_id, json!("Different")),
    );
    let (record, issues) = convert_content(source("download", "file", payload), seed(referenced));
    assert!(record.is_none());
    for code in [
        MigrationPreflightIssueCode::MissingMediaVersion,
        MigrationPreflightIssueCode::TypeFieldMismatch,
    ] {
        assert!(issues.iter().any(|issue| issue.code == code));
    }
}
