use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::models::{
    BodyBlock, CmsContentKind, CollectionPresentation, ContentBlock, ContentRelationReference,
    ContentWidth, CtaBlock, CtaVariant, EditorialAction, FeatureGridBlock, FeatureItem, HeroBlock,
    HeroVariant, PageComposition, RelationCollectionBlock, RelationTargetReference, TiptapDocument,
};

use super::{
    composition_validation::{
        check_object_keys, check_optional_strings, check_unknown_slots, optional_string,
        warn_server_derived_slots,
    },
    conversion::{blocking, issue, IssueContext},
    support::{link_target, stable_id},
    types::{CmsPreflightIssue, CmsPreflightIssueCode, CmsPreflightSeverity},
};

pub(super) fn convert_page_slots(
    page_slots: Option<&Map<String, Value>>,
    kind: CmsContentKind,
    context: IssueContext,
    document: &Option<TiptapDocument>,
    allows_lossy_placeholder_cleanup: bool,
    issues: &mut Vec<CmsPreflightIssue>,
) -> (PageComposition, Vec<ContentRelationReference>) {
    let Some(slots) = page_slots else {
        return (
            PageComposition {
                blocks: body_block(context, document),
            },
            Vec::new(),
        );
    };
    let mut blocks = Vec::new();
    let root_eyebrow = optional_string(slots.get("eyebrow"));
    if slots
        .get("eyebrow")
        .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            "pageSlots.eyebrow",
            "Page eyebrow must be a string or null.",
        ));
    }
    if let Some(value) = slots.get("hero") {
        if let Some(block) = hero_block(
            value,
            root_eyebrow.as_deref(),
            context,
            allows_lossy_placeholder_cleanup,
            issues,
        ) {
            blocks.push(block);
        }
    } else if let Some(eyebrow) = root_eyebrow {
        blocks.push(ContentBlock::Hero(HeroBlock {
            id: stable_id(context, "hero"),
            eyebrow: Some(eyebrow),
            heading: None,
            lead: None,
            media: None,
            actions: Vec::new(),
            variant: HeroVariant::Standard,
        }));
    }
    blocks.extend(body_block(context, document));
    if let Some(value) = slots.get("sections") {
        if let Some(block) = sections_block(value, context, issues) {
            blocks.push(block);
        }
    }
    let relations = slots
        .get("relationships")
        .map(|value| embedded_relations(value, context, issues))
        .unwrap_or_default();
    if !relations.is_empty() {
        blocks.push(ContentBlock::RelationCollection(RelationCollectionBlock {
            id: stable_id(context, "relationships"),
            heading: None,
            relation_ids: relations.iter().map(|relation| relation.id).collect(),
            presentation: CollectionPresentation::Cards,
        }));
    }
    if let Some(value) = slots.get("primaryCta") {
        if let Some(block) = cta_block(value, context, issues) {
            blocks.push(block);
        }
    }
    convert_exact_blocks(slots, context, &mut blocks, issues);
    warn_server_derived_slots(slots, context, issues);
    check_unknown_slots(slots, kind, context, issues);
    (PageComposition { blocks }, relations)
}

fn hero_block(
    value: &Value,
    root_eyebrow: Option<&str>,
    context: IssueContext,
    allows_lossy_placeholder_cleanup: bool,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<ContentBlock> {
    let Some(object) = value.as_object() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            "pageSlots.hero",
            "Hero must be an object.",
        ));
        return None;
    };
    check_object_keys(
        object,
        &["eyebrow", "title", "description"],
        context,
        "pageSlots.hero",
        issues,
    );
    check_optional_strings(
        object,
        &["eyebrow", "title", "description"],
        context,
        "pageSlots.hero",
        issues,
    );
    let hero_eyebrow = optional_string(object.get("eyebrow"));
    if root_eyebrow.is_some()
        && hero_eyebrow.as_deref().is_some()
        && root_eyebrow != hero_eyebrow.as_deref()
    {
        issues.push(if allows_lossy_placeholder_cleanup {
            issue(
                CmsPreflightSeverity::Warning,
                CmsPreflightIssueCode::UnknownBlock,
                context.source,
                Some(context.entity_id),
                Some(context.revision),
                "pageSlots.eyebrow",
                "Development fixture uses the more specific hero eyebrow and drops the conflicting top-level value.",
            )
        } else {
            blocking(
                context,
                CmsPreflightIssueCode::UnknownBlock,
                "pageSlots.eyebrow",
                "Top-level and hero eyebrow values conflict; precedence cannot be guessed.",
            )
        });
    }
    Some(ContentBlock::Hero(HeroBlock {
        id: stable_id(context, "hero"),
        eyebrow: hero_eyebrow.or_else(|| root_eyebrow.map(Into::into)),
        heading: optional_string(object.get("title")),
        lead: optional_string(object.get("description")),
        media: None,
        actions: Vec::new(),
        variant: HeroVariant::Standard,
    }))
}

fn body_block(context: IssueContext, document: &Option<TiptapDocument>) -> Vec<ContentBlock> {
    document
        .as_ref()
        .filter(|document| !document.content.is_empty())
        .map(|_| {
            vec![ContentBlock::Body(BodyBlock {
                id: stable_id(context, "document"),
                width: ContentWidth::Standard,
            })]
        })
        .unwrap_or_default()
}

fn sections_block(
    value: &Value,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<ContentBlock> {
    let Some(sections) = value.as_array() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            "pageSlots.sections",
            "Sections must be an array.",
        ));
        return None;
    };
    let mut items = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        let path = format!("pageSlots.sections.{index}");
        let Some(object) = section.as_object() else {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::UnknownBlock,
                &path,
                "Section must be an object.",
            ));
            continue;
        };
        if object.contains_key("type") {
            issues.push(blocking(context, CmsPreflightIssueCode::UnknownBlock, &format!("{path}.type"), "Typed legacy section cannot be assigned to a V2 block without an exact contract match."));
            continue;
        }
        check_object_keys(
            object,
            &["id", "eyebrow", "title", "description", "links"],
            context,
            &path,
            issues,
        );
        check_optional_strings(
            object,
            &["eyebrow", "title", "description"],
            context,
            &path,
            issues,
        );
        if object
            .get("links")
            .is_some_and(|value| !value.as_array().is_some_and(|links| links.is_empty()))
        {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::UnknownBlock,
                &format!("{path}.links"),
                "Section links have no lossless Feature Grid representation.",
            ));
        }
        if optional_string(object.get("eyebrow")).is_some() {
            issues.push(issue(CmsPreflightSeverity::Warning, CmsPreflightIssueCode::InvalidLegacyPayload, context.source, Some(context.entity_id), Some(context.revision), &format!("{path}.eyebrow"), "Section eyebrow is presentation-only and is explicitly omitted from the V2 Feature Grid."));
        }
        let Some(title) = optional_string(object.get("title")) else {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::UnknownBlock,
                &format!("{path}.title"),
                "Section title is required for Feature Grid conversion.",
            ));
            continue;
        };
        items.push(FeatureItem {
            id: stable_id(context, &format!("sections.{index}")),
            title,
            description: optional_string(object.get("description")),
            icon: None,
        });
    }
    (!items.is_empty()).then(|| {
        ContentBlock::FeatureGrid(FeatureGridBlock {
            id: stable_id(context, "sections"),
            heading: None,
            items,
        })
    })
}

fn cta_block(
    value: &Value,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<ContentBlock> {
    let Some(object) = value.as_object() else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            "pageSlots.primaryCta",
            "CTA must be an object.",
        ));
        return None;
    };
    check_object_keys(
        object,
        &["eyebrow", "title", "description", "label", "href"],
        context,
        "pageSlots.primaryCta",
        issues,
    );
    check_optional_strings(
        object,
        &["eyebrow", "title", "description", "label", "href"],
        context,
        "pageSlots.primaryCta",
        issues,
    );
    let (Some(heading), Some(label), Some(href)) = (
        optional_string(object.get("title")),
        optional_string(object.get("label")),
        optional_string(object.get("href")),
    ) else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::UnknownBlock,
            "pageSlots.primaryCta",
            "CTA title, label, and href are required.",
        ));
        return None;
    };
    let Some(target) = link_target(&href) else {
        issues.push(blocking(
            context,
            CmsPreflightIssueCode::MissingRelationTarget,
            "pageSlots.primaryCta.href",
            "CTA target is neither an absolute route nor credential-free HTTPS URL.",
        ));
        return None;
    };
    Some(ContentBlock::Cta(CtaBlock {
        id: stable_id(context, "primaryCta"),
        eyebrow: optional_string(object.get("eyebrow")),
        heading,
        body: optional_string(object.get("description")),
        action: EditorialAction { label, target },
        variant: CtaVariant::Standard,
    }))
}

fn embedded_relations(
    value: &Value,
    context: IssueContext,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Vec<ContentRelationReference> {
    let mut grouped = Vec::new();
    match value {
        Value::Array(items) => grouped.extend(items.iter().map(|item| ("related", item))),
        Value::Object(groups) => {
            for (slot, value) in groups {
                if let Some(items) = value.as_array() {
                    grouped.extend(items.iter().map(|item| (slot.as_str(), item)));
                } else {
                    issues.push(blocking(
                        context,
                        CmsPreflightIssueCode::InvalidRelation,
                        &format!("pageSlots.relationships.{slot}"),
                        "Relationship group must be an array.",
                    ));
                }
            }
        }
        _ => issues.push(blocking(
            context,
            CmsPreflightIssueCode::InvalidRelation,
            "pageSlots.relationships",
            "Relationships must be an array or object of arrays.",
        )),
    }
    let mut result = Vec::new();
    for (index, (slot, value)) in grouped.into_iter().enumerate() {
        let object = value.as_object();
        if let Some(object) = object {
            check_object_keys(
                object,
                &["entityType", "entityId", "id"],
                context,
                &format!("pageSlots.relationships.{index}"),
                issues,
            );
        }
        let target_type = object
            .and_then(|value| value.get("entityType"))
            .and_then(Value::as_str);
        let target_id = object
            .and_then(|value| value.get("entityId").or_else(|| value.get("id")))
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        let Some(target_id) = target_id else {
            issues.push(blocking(
                context,
                CmsPreflightIssueCode::MissingRelationTarget,
                &format!("pageSlots.relationships.{index}"),
                "Relationship has no stable UUID; slugs and paths are not guessed.",
            ));
            continue;
        };
        let target = match target_type {
            Some("content") => RelationTargetReference::Content {
                content_id: target_id,
            },
            Some("product") => RelationTargetReference::Product {
                product_id: target_id,
            },
            _ => {
                issues.push(blocking(
                    context,
                    CmsPreflightIssueCode::InvalidRelation,
                    &format!("pageSlots.relationships.{index}.entityType"),
                    "Relationship target type is not recognized.",
                ));
                continue;
            }
        };
        result.push(ContentRelationReference {
            id: stable_id(context, &format!("relationships.{slot}.{target_id}")),
            slot: slot.into(),
            target,
        });
    }
    result
}

fn convert_exact_blocks(
    slots: &Map<String, Value>,
    context: IssueContext,
    blocks: &mut Vec<ContentBlock>,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for key in [
        "media",
        "featureGrid",
        "evidence",
        "faqCollection",
        "downloadAsset",
        "contactBlock",
    ] {
        let Some(value) = slots.get(key) else {
            continue;
        };
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object
                .entry("id")
                .or_insert_with(|| json!(stable_id(context, key)));
            object.insert("type".into(), json!(key));
        }
        match serde_json::from_value::<ContentBlock>(value) {
            Ok(block) => blocks.push(block),
            Err(_) => issues.push(blocking(
                context,
                CmsPreflightIssueCode::UnknownBlock,
                &format!("pageSlots.{key}"),
                "Legacy block does not satisfy the strict V2 block contract.",
            )),
        }
    }
}
