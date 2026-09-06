use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{conversion::issue, types::*};

pub(super) fn stable_media_version_id(asset_id: Uuid) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"airtek-cms-v2-media-version\0");
    digest.update(asset_id.as_bytes());
    let hash = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub(super) fn convert_references(
    snapshot: &LegacySnapshot,
    issues: &mut Vec<CmsPreflightIssue>,
) -> (
    Vec<CmsV2MediaVersionCandidate>,
    Vec<CmsV2RelationCandidate>,
    Vec<CmsV2AssetReferenceCandidate>,
) {
    let content_ids = snapshot
        .content_entries
        .iter()
        .map(|entry| entry.id)
        .collect::<BTreeSet<_>>();
    let content_revisions = snapshot
        .content_revisions
        .iter()
        .map(|revision| (revision.content_id, revision.revision))
        .collect::<BTreeSet<_>>();
    let product_ids = snapshot
        .products
        .iter()
        .map(|value| value.id)
        .collect::<BTreeSet<_>>();
    let information_revisions = snapshot
        .general_information_revisions
        .iter()
        .map(|revision| (revision.general_information_id, revision.revision))
        .collect::<BTreeSet<_>>();
    let media = snapshot
        .media_assets
        .iter()
        .map(|asset| (asset.id, asset))
        .collect::<BTreeMap<_, _>>();

    let media_versions = snapshot
        .media_assets
        .iter()
        .map(|asset| CmsV2MediaVersionCandidate {
            asset_id: asset.id,
            version_id: stable_media_version_id(asset.id),
        })
        .collect();
    let relations = convert_content_relations(snapshot, &content_ids, &product_ids, issues);
    let asset_references = snapshot
        .asset_references
        .iter()
        .filter_map(|reference| {
            convert_asset_reference(
                snapshot,
                reference,
                &content_revisions,
                &information_revisions,
                &media,
                issues,
            )
        })
        .collect();
    (media_versions, relations, asset_references)
}

fn convert_content_relations(
    snapshot: &LegacySnapshot,
    content_ids: &BTreeSet<Uuid>,
    product_ids: &BTreeSet<Uuid>,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Vec<CmsV2RelationCandidate> {
    snapshot
        .content_relations
        .iter()
        .filter_map(|relation| {
            let path = format!(
                "{}:{}:{}:{}:{}",
                relation.from_type,
                relation.from_id,
                relation.relation_type,
                relation.to_type,
                relation.to_id
            );
            let mut valid = true;
            if relation.from_type != "content" {
                valid = false;
                issues.push(issue(
                    CmsPreflightSeverity::Blocking,
                    CmsPreflightIssueCode::InvalidRelation,
                    CmsPreflightSource::Relation,
                    Some(relation.from_id),
                    None,
                    &path,
                    "Only content relation sources are supported by CMS V2.",
                ));
            } else if !content_ids.contains(&relation.from_id) {
                valid = false;
                issues.push(issue(
                    CmsPreflightSeverity::Blocking,
                    CmsPreflightIssueCode::MissingRelationTarget,
                    CmsPreflightSource::Relation,
                    Some(relation.from_id),
                    None,
                    &path,
                    "Relation source content does not exist.",
                ));
            }
            match relation.to_type.as_str() {
                "content" if !content_ids.contains(&relation.to_id) => {
                    valid = false;
                    issues.push(issue(
                        CmsPreflightSeverity::Blocking,
                        CmsPreflightIssueCode::MissingRelationTarget,
                        CmsPreflightSource::Relation,
                        Some(relation.from_id),
                        None,
                        &path,
                        "Relation target content does not exist.",
                    ));
                }
                "product" if !product_ids.contains(&relation.to_id) => {
                    valid = false;
                    issues.push(issue(
                        CmsPreflightSeverity::Blocking,
                        CmsPreflightIssueCode::MissingRelationTarget,
                        CmsPreflightSource::Relation,
                        Some(relation.from_id),
                        None,
                        &path,
                        "Relation target product does not exist.",
                    ));
                }
                "content" | "product" => {}
                _ => {
                    valid = false;
                    issues.push(issue(
                        CmsPreflightSeverity::Blocking,
                        CmsPreflightIssueCode::InvalidRelation,
                        CmsPreflightSource::Relation,
                        Some(relation.from_id),
                        None,
                        &path,
                        "Relation target type is not recognized.",
                    ));
                }
            }
            valid.then(|| CmsV2RelationCandidate {
                from_id: relation.from_id,
                relation_type: relation.relation_type.clone(),
                to_type: relation.to_type.clone(),
                to_id: relation.to_id,
                sort_order: relation.sort_order,
            })
        })
        .collect()
}

fn convert_asset_reference(
    snapshot: &LegacySnapshot,
    reference: &LegacyAssetReference,
    content_revisions: &BTreeSet<(Uuid, i64)>,
    information_revisions: &BTreeSet<(Uuid, i64)>,
    media: &BTreeMap<Uuid, &LegacyMediaAsset>,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<CmsV2AssetReferenceCandidate> {
    let owner = match (
        reference.content_id.zip(reference.content_revision),
        reference.product_id.zip(reference.product_revision),
        reference
            .general_information_id
            .zip(reference.general_information_revision),
    ) {
        (Some(owner), None, None) => Some(("content", owner)),
        (None, Some(owner), None) => Some(("product", owner)),
        (None, None, Some(owner)) => Some(("generalInformation", owner)),
        _ => None,
    };
    let Some((owner_type, (owner_id, owner_revision))) = owner else {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::InvalidRelation,
            CmsPreflightSource::Media,
            Some(reference.id),
            None,
            "owner",
            "Asset reference does not have exactly one revision-scoped owner.",
        ));
        return None;
    };
    let owner_exists = match owner_type {
        "content" => content_revisions.contains(&(owner_id, owner_revision)),
        "generalInformation" => information_revisions.contains(&(owner_id, owner_revision)),
        // The legacy foreign key validates the revision; this snapshot still
        // validates that the stable product target itself exists.
        "product" => snapshot
            .products
            .iter()
            .any(|product| product.id == owner_id),
        _ => false,
    };
    if !owner_exists {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::InvalidRelation,
            CmsPreflightSource::Media,
            Some(reference.id),
            Some(owner_revision),
            "owner",
            "Asset reference owner revision does not exist.",
        ));
        return None;
    }
    const USAGES: &[&str] = &[
        "hero",
        "cover",
        "gallery",
        "inline",
        "download",
        "datasheet",
        "cad",
        "certificate",
        "logo",
        "favicon",
        "social",
        "other",
    ];
    if !USAGES.contains(&reference.usage.as_str()) {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::InvalidRelation,
            CmsPreflightSource::Media,
            Some(reference.id),
            Some(owner_revision),
            "usage",
            "Asset reference usage is not recognized.",
        ));
        return None;
    }
    let Some(asset) = media.get(&reference.media_asset_id) else {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::MissingMediaAsset,
            CmsPreflightSource::Media,
            Some(reference.id),
            Some(owner_revision),
            "mediaAssetId",
            "Referenced media asset does not exist.",
        ));
        return None;
    };
    let published = owner_is_published(snapshot, owner_type, owner_id, owner_revision);
    if asset.deleted_at.is_some() || asset.scan_status != "clean" || asset.access_level != "public"
    {
        issues.push(issue(
            if published {
                CmsPreflightSeverity::Blocking
            } else {
                CmsPreflightSeverity::Warning
            },
            CmsPreflightIssueCode::MissingMediaVersion,
            CmsPreflightSource::Media,
            Some(reference.id),
            Some(owner_revision),
            "mediaAssetId",
            "Referenced media is deleted, not clean, or not public.",
        ));
    }
    if asset.media_type.starts_with("image/")
        && reference.usage != "favicon"
        && reference
            .alt_text
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        issues.push(issue(
            if published {
                CmsPreflightSeverity::Blocking
            } else {
                CmsPreflightSeverity::Warning
            },
            CmsPreflightIssueCode::TypeFieldMismatch,
            CmsPreflightSource::Media,
            Some(reference.id),
            Some(owner_revision),
            "altText",
            "Image reference has no alternative text or decorative declaration.",
        ));
    }
    Some(CmsV2AssetReferenceCandidate {
        source_id: reference.id,
        asset_id: reference.media_asset_id,
        version_id: stable_media_version_id(reference.media_asset_id),
        owner_type: owner_type.into(),
        owner_id,
        owner_revision,
        usage: reference.usage.clone(),
        locale: reference.locale.clone(),
        alt_text: reference.alt_text.clone(),
        sort_order: reference.sort_order,
    })
}

fn owner_is_published(
    snapshot: &LegacySnapshot,
    owner_type: &str,
    owner_id: Uuid,
    revision: i64,
) -> bool {
    match owner_type {
        "content" => snapshot
            .content_entries
            .iter()
            .any(|entry| entry.id == owner_id && entry.published_revision == Some(revision)),
        "generalInformation" => snapshot
            .general_information
            .iter()
            .any(|entry| entry.id == owner_id && entry.published_revision == Some(revision)),
        "product" => snapshot
            .products
            .iter()
            .any(|product| product.id == owner_id && product.published_revision == Some(revision)),
        _ => false,
    }
}
