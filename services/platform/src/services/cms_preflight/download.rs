use std::collections::BTreeSet;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::models::{
    AssetVersionReference, ContentBlock, ContentRelationReference, DownloadAssetBlock,
    PageComposition, RelationTargetReference,
};

use super::{
    conversion::{blocking, IssueContext},
    support::stable_id,
    types::{CmsPreflightIssue, CmsPreflightIssueCode},
};

pub(super) fn model_relations(
    attrs: Option<&Map<String, Value>>,
    seed: &Map<String, Value>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Vec<ContentRelationReference> {
    let Some(value) = attrs.and_then(|attrs| attrs.get("applicableModels")) else {
        return Vec::new();
    };
    let Some(values) = value.as_array() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidRelation,
            "body.doc.attrs.applicableModels",
            "Download applicableModels must be an array of stable product UUIDs.",
        ));
        return Vec::new();
    };
    let known = seed
        .get("productIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|value| Uuid::parse_str(value).ok())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut relations = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let product_id = value.as_str().and_then(|value| Uuid::parse_str(value).ok());
        let Some(product_id) = product_id else {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::InvalidRelation,
                &format!("body.doc.attrs.applicableModels.{index}"),
                "Legacy model labels cannot be guessed into product IDs; replace this value with a stable UUID.",
            ));
            continue;
        };
        if !known.contains(&product_id) {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::MissingRelationTarget,
                &format!("body.doc.attrs.applicableModels.{index}"),
                "Download applicable model does not reference an existing product.",
            ));
            continue;
        }
        if seen.insert(product_id) {
            relations.push(ContentRelationReference {
                id: stable_id(context, &format!("applicableModels.{product_id}")),
                slot: "applicableModels".into(),
                target: RelationTargetReference::Product { product_id },
            });
        }
    }
    relations
}

pub(super) fn merge_relations(
    embedded: &mut Vec<ContentRelationReference>,
    current: Vec<ContentRelationReference>,
) {
    for relation in current {
        let duplicate = embedded.iter().any(|candidate| {
            candidate.slot == relation.slot && candidate.target == relation.target
        });
        if !duplicate {
            embedded.push(relation);
        }
    }
}

pub(super) fn reconcile_download_asset(
    composition: &mut PageComposition,
    attrs: Option<&Map<String, Value>>,
    seed: &Map<String, Value>,
    title: &str,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let seed_asset = seed
        .get("asset")
        .cloned()
        .and_then(|value| serde_json::from_value::<AssetVersionReference>(value).ok());
    let description = attrs
        .and_then(|value| value.get("fileDescription"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let existing = composition.blocks.iter_mut().find_map(|block| match block {
        ContentBlock::DownloadAsset(value) => Some(value),
        _ => None,
    });
    match (existing, seed_asset) {
        (Some(block), Some(seed_asset)) => {
            if block.asset != seed_asset {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::MissingMediaVersion,
                    "pageSlots.downloadAsset.asset",
                    "Embedded download asset disagrees with the authoritative asset reference.",
                ));
            }
            reconcile_description(block, description, context, issues);
        }
        (Some(block), None) => reconcile_description(block, description, context, issues),
        (None, Some(asset)) => {
            composition
                .blocks
                .push(ContentBlock::DownloadAsset(DownloadAssetBlock {
                    id: stable_id(context, "downloadAssetReference"),
                    asset,
                    label: title.into(),
                    description: description.map(Into::into),
                }))
        }
        (None, None) if description.is_some() => issues.push(blocking(
            context,
            CmsPreflightIssueCode::MissingMediaAsset,
            "body.doc.attrs.fileDescription",
            "Download fileDescription cannot be preserved without a mapped DownloadAssetBlock.",
        )),
        (None, None) => {}
    }
}

fn reconcile_description(
    block: &mut DownloadAssetBlock,
    legacy: Option<&str>,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let Some(legacy) = legacy else { return };
    match block.description.as_deref().map(str::trim) {
        None | Some("") => block.description = Some(legacy.into()),
        Some(existing) if existing == legacy => {}
        Some(_) => issues.push(blocking(
            context,
            CmsPreflightIssueCode::TypeFieldMismatch,
            "body.doc.attrs.fileDescription",
            "Embedded download description conflicts with body.doc.attrs.fileDescription.",
        )),
    }
}
