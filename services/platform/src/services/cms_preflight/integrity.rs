use std::collections::{BTreeMap, BTreeSet};

use crate::models::{
    AssetVersionReference, ContentBlock, ContentDraftV2, ContentTypeFields, EditorialAction,
    LinkTargetReference, MediaUseReference, NavigationItem, RelationTargetReference,
};

use super::{conversion::issue, types::*};

pub(super) fn validate_candidate_targets(
    records: &mut Vec<CmsPreflightRecord>,
    snapshot: &LegacySnapshot,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let content_ids = snapshot
        .known_content_ids
        .iter()
        .copied()
        .chain(snapshot.content_entries.iter().map(|entry| entry.id))
        .collect::<BTreeSet<_>>();
    let product_ids = snapshot
        .products
        .iter()
        .map(|product| product.id)
        .collect::<BTreeSet<_>>();
    let media = snapshot
        .media_assets
        .iter()
        .map(|asset| (asset.id, asset))
        .collect::<BTreeMap<_, _>>();
    records.retain(|record| {
        validate_record(record, snapshot, &content_ids, &product_ids, &media, issues)
    });
}

fn validate_record(
    record: &CmsPreflightRecord,
    snapshot: &LegacySnapshot,
    content_ids: &BTreeSet<uuid::Uuid>,
    product_ids: &BTreeSet<uuid::Uuid>,
    media: &BTreeMap<uuid::Uuid, &LegacyMediaAsset>,
    issues: &mut Vec<CmsPreflightIssue>,
) -> bool {
    let source = if record.candidate.kind == crate::models::CmsContentKind::GeneralInformation {
        CmsPreflightSource::GeneralInformation
    } else if record.role == CmsPreflightRecordRole::Working {
        CmsPreflightSource::Content
    } else {
        CmsPreflightSource::ContentRevision
    };
    let mut valid = true;
    for (index, relation) in record.candidate.relations.iter().enumerate() {
        let exists = match relation.target {
            RelationTargetReference::Content { content_id } => content_ids.contains(&content_id),
            RelationTargetReference::Product { product_id } => product_ids.contains(&product_id),
        };
        if !exists {
            valid = false;
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::MissingRelationTarget,
                source,
                Some(record.entity_id),
                Some(record.source_revision),
                &format!("relations.{index}.target"),
                "Candidate relation target does not exist in the read-only snapshot.",
            ));
        }
    }
    for (path, content_id) in content_link_targets(&record.candidate) {
        if !content_ids.contains(&content_id) {
            valid = false;
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::MissingRelationTarget,
                source,
                Some(record.entity_id),
                Some(record.source_revision),
                &path,
                "Candidate link target references content absent from the read-only snapshot.",
            ));
        }
    }
    for (path, asset) in asset_references(&record.candidate) {
        let Some(legacy) = media.get(&asset.asset_id) else {
            valid = false;
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::MissingMediaAsset,
                source,
                Some(record.entity_id),
                Some(record.source_revision),
                &path,
                "Candidate media reference points to a missing legacy asset.",
            ));
            continue;
        };
        if legacy.deleted_at.is_some() {
            let published = record_is_published(record, snapshot);
            valid &= !published;
            issues.push(issue(
                if published {
                    CmsPreflightSeverity::Blocking
                } else {
                    CmsPreflightSeverity::Warning
                },
                CmsPreflightIssueCode::MissingMediaVersion,
                source,
                Some(record.entity_id),
                Some(record.source_revision),
                &path,
                "Candidate media was deleted.",
            ));
        }
    }
    valid
}

fn content_link_targets(draft: &ContentDraftV2) -> Vec<(String, uuid::Uuid)> {
    let mut values = Vec::new();
    for (index, block) in draft.composition.blocks.iter().enumerate() {
        match block {
            ContentBlock::Hero(hero) => {
                for (action_index, action) in hero.actions.iter().enumerate() {
                    push_action(
                        &mut values,
                        &format!("composition.blocks.{index}.actions.{action_index}.target"),
                        action,
                    );
                }
            }
            ContentBlock::Cta(cta) => push_action(
                &mut values,
                &format!("composition.blocks.{index}.action.target"),
                &cta.action,
            ),
            ContentBlock::ContactBlock(contact) => {
                if let Some(action) = &contact.action {
                    push_action(
                        &mut values,
                        &format!("composition.blocks.{index}.action.target"),
                        action,
                    );
                }
            }
            _ => {}
        }
    }
    match &draft.type_fields {
        ContentTypeFields::GeneralInformation(fields) => {
            if let Some(action) = &fields.navigation_cta {
                push_action(&mut values, "typeFields.navigationCta.target", action);
            }
        }
        ContentTypeFields::Navigation(fields) => {
            push_navigation(&mut values, "typeFields.items", &fields.items);
        }
        ContentTypeFields::Footer(fields) => {
            for (index, column) in fields.columns.iter().enumerate() {
                push_navigation(
                    &mut values,
                    &format!("typeFields.columns.{index}.links"),
                    &column.links,
                );
            }
            push_navigation(&mut values, "typeFields.legalLinks", &fields.legal_links);
        }
        _ => {}
    }
    values
}

fn push_navigation(values: &mut Vec<(String, uuid::Uuid)>, path: &str, items: &[NavigationItem]) {
    for (index, item) in items.iter().enumerate() {
        let item_path = format!("{path}.{index}");
        if let Some(target) = &item.target {
            push_target(values, &format!("{item_path}.target"), target);
        }
        push_navigation(values, &format!("{item_path}.children"), &item.children);
    }
}

fn push_action(values: &mut Vec<(String, uuid::Uuid)>, path: &str, action: &EditorialAction) {
    push_target(values, path, &action.target);
}

fn push_target(values: &mut Vec<(String, uuid::Uuid)>, path: &str, target: &LinkTargetReference) {
    if let LinkTargetReference::Content { content_id } = target {
        values.push((path.into(), *content_id));
    }
}

fn record_is_published(record: &CmsPreflightRecord, snapshot: &LegacySnapshot) -> bool {
    if record.candidate.kind == crate::models::CmsContentKind::GeneralInformation {
        snapshot.general_information.iter().any(|entry| {
            entry.id == record.entity_id && entry.published_revision == Some(record.source_revision)
        })
    } else {
        snapshot.content_entries.iter().any(|entry| {
            entry.id == record.entity_id && entry.published_revision == Some(record.source_revision)
        })
    }
}

fn asset_references(draft: &ContentDraftV2) -> Vec<(String, &AssetVersionReference)> {
    let mut values = Vec::new();
    for (index, block) in draft.composition.blocks.iter().enumerate() {
        match block {
            ContentBlock::Hero(value) => push_media(
                &mut values,
                &format!("composition.blocks.{index}.media.asset"),
                value.media.as_ref(),
            ),
            ContentBlock::Media(value) => values.push((
                format!("composition.blocks.{index}.media.asset"),
                &value.media.asset,
            )),
            ContentBlock::FeatureGrid(value) => {
                for (item_index, item) in value.items.iter().enumerate() {
                    push_media(
                        &mut values,
                        &format!("composition.blocks.{index}.items.{item_index}.icon.asset"),
                        item.icon.as_ref(),
                    );
                }
            }
            ContentBlock::DownloadAsset(value) => {
                values.push((format!("composition.blocks.{index}.asset"), &value.asset))
            }
            _ => {}
        }
    }
    match &draft.type_fields {
        ContentTypeFields::Article(value) | ContentTypeFields::News(value) => {
            push_media(&mut values, "typeFields.cover.asset", value.cover.as_ref());
        }
        ContentTypeFields::GeneralInformation(value) => push_media(
            &mut values,
            "typeFields.defaultSeo.socialImage.asset",
            value.default_seo.social_image.as_ref(),
        ),
        _ => {}
    }
    push_media(
        &mut values,
        "seo.socialImage.asset",
        draft.seo.social_image.as_ref(),
    );
    values
}

fn push_media<'a>(
    values: &mut Vec<(String, &'a AssetVersionReference)>,
    path: &str,
    media: Option<&'a MediaUseReference>,
) {
    if let Some(media) = media {
        values.push((path.into(), &media.asset));
    }
}
