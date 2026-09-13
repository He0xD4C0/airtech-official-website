use std::collections::HashSet;

use serde::Serialize;

use crate::models::{
    CmsBodyPolicy, CmsContentKind, ContentBlock, ContentBlockKind, ContentDraftV2,
    ContentTemplateKey, ContentTypeFields, MediaUseReference, CMS_V2_SCHEMA_VERSION,
};

use super::template_definition;

mod contract_values;
mod links;
mod rich_text;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CmsTemplateValidationCode {
    UnsupportedSchemaVersion,
    TemplateKindMismatch,
    TypeFieldsKindMismatch,
    MissingSlug,
    UnexpectedSlug,
    MissingBody,
    ForbiddenBody,
    UnplacedBody,
    EmptyBodyBlock,
    RequiredBlockMissing,
    DisallowedBlock,
    DuplicateBlockId,
    DuplicateRelationId,
    MissingRelationReference,
    InvalidRichText,
    EmptyTitle,
    IndexingNotAllowed,
    MissingMediaAlt,
    InvalidLink,
    InvalidContractValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmsTemplateValidationPhase {
    Structural,
    Publish,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmsTemplateValidationIssue {
    pub code: CmsTemplateValidationCode,
    pub path: String,
    pub message: String,
}

pub fn validate_content_draft(
    draft: &ContentDraftV2,
    phase: CmsTemplateValidationPhase,
) -> Vec<CmsTemplateValidationIssue> {
    let mut issues = Vec::new();
    if draft.schema_version != CMS_V2_SCHEMA_VERSION {
        push(
            &mut issues,
            CmsTemplateValidationCode::UnsupportedSchemaVersion,
            "schemaVersion",
            "The draft does not use the supported CMS schema version.",
        );
    }

    let Some(template) = template_definition(draft.template_key) else {
        push(
            &mut issues,
            CmsTemplateValidationCode::TemplateKindMismatch,
            "templateKey",
            "The template is not present in the controlled registry.",
        );
        return issues;
    };
    if draft.kind != template.content_kind {
        push(
            &mut issues,
            CmsTemplateValidationCode::TemplateKindMismatch,
            "kind",
            "The content kind is incompatible with the selected template.",
        );
    }
    if type_fields_kind(&draft.type_fields) != draft.kind {
        push(
            &mut issues,
            CmsTemplateValidationCode::TypeFieldsKindMismatch,
            "typeFields.type",
            "The type-fields discriminator must equal the draft content kind.",
        );
    }
    match (template.routable, draft.slug.as_deref(), phase) {
        (true, None | Some(""), CmsTemplateValidationPhase::Publish) => push(
            &mut issues,
            CmsTemplateValidationCode::MissingSlug,
            "slug",
            "Routable content requires a non-empty slug.",
        ),
        (false, Some(_), _) => push(
            &mut issues,
            CmsTemplateValidationCode::UnexpectedSlug,
            "slug",
            "Configuration content must not carry a public slug.",
        ),
        _ => {}
    }

    let has_body = draft
        .body
        .as_ref()
        .is_some_and(|document| !document.content.is_empty());
    match (template.body_policy, phase) {
        (CmsBodyPolicy::Required, CmsTemplateValidationPhase::Publish) if !has_body => push(
            &mut issues,
            CmsTemplateValidationCode::MissingBody,
            "body",
            "This template requires a non-empty Tiptap body.",
        ),
        (CmsBodyPolicy::Forbidden, _) if draft.body.is_some() => push(
            &mut issues,
            CmsTemplateValidationCode::ForbiddenBody,
            "body",
            "This template does not accept a Tiptap body.",
        ),
        _ => {}
    }

    if let Some(document) = &draft.body {
        for (path, message) in rich_text::validate(document) {
            push(
                &mut issues,
                CmsTemplateValidationCode::InvalidRichText,
                &path,
                &message,
            );
        }
    }
    if let ContentTypeFields::Faq(fields) = &draft.type_fields {
        for (index, item) in fields.items.iter().enumerate() {
            for (path, message) in
                rich_text::validate_at(&item.answer, &format!("typeFields.items.{index}.answer"))
            {
                push(
                    &mut issues,
                    CmsTemplateValidationCode::InvalidRichText,
                    &path,
                    &message,
                );
            }
        }
    }
    for (path, message) in links::validate(draft) {
        push(
            &mut issues,
            CmsTemplateValidationCode::InvalidLink,
            &path,
            &message,
        );
    }
    for (path, message) in contract_values::validate(draft) {
        push(
            &mut issues,
            CmsTemplateValidationCode::InvalidContractValue,
            &path,
            &message,
        );
    }

    let mut block_ids = HashSet::new();
    let block_kinds = draft
        .composition
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let kind = block.kind();
            if !block_ids.insert(block_id(block)) {
                push(
                    &mut issues,
                    CmsTemplateValidationCode::DuplicateBlockId,
                    &format!("composition.blocks.{index}.id"),
                    "Composition block IDs must be unique within a draft.",
                );
            }
            if !template.allowed_blocks.contains(&kind) {
                push(
                    &mut issues,
                    CmsTemplateValidationCode::DisallowedBlock,
                    &format!("composition.blocks.{index}.type"),
                    "The block is not allowed by the selected template.",
                );
            }
            kind
        })
        .collect::<Vec<_>>();
    if phase == CmsTemplateValidationPhase::Publish {
        for required in &template.required_blocks {
            if !block_kinds.contains(required) {
                push(
                    &mut issues,
                    CmsTemplateValidationCode::RequiredBlockMissing,
                    "composition.blocks",
                    &format!("The template requires a {required:?} block."),
                );
            }
        }
    }
    let has_body_block = block_kinds.contains(&ContentBlockKind::Body);
    if phase == CmsTemplateValidationPhase::Publish && has_body && !has_body_block {
        push(
            &mut issues,
            CmsTemplateValidationCode::UnplacedBody,
            "composition.blocks",
            "A non-empty body requires a body block to place it in the composition.",
        );
    }
    if phase == CmsTemplateValidationPhase::Publish && has_body_block && !has_body {
        push(
            &mut issues,
            CmsTemplateValidationCode::EmptyBodyBlock,
            "composition.blocks",
            "A body block requires a non-empty Tiptap body.",
        );
    }

    validate_relations(draft, &mut issues);
    if phase == CmsTemplateValidationPhase::Publish {
        validate_publish_rules(draft, template.routable, &mut issues);
    }
    issues
}

fn validate_publish_rules(
    draft: &ContentDraftV2,
    routable: bool,
    issues: &mut Vec<CmsTemplateValidationIssue>,
) {
    if draft.title.trim().is_empty() {
        push(
            issues,
            CmsTemplateValidationCode::EmptyTitle,
            "title",
            "Published content requires a non-empty title.",
        );
    }
    let forced_noindex = draft.is_placeholder
        || !routable
        || matches!(
            draft.template_key,
            ContentTemplateKey::Compare | ContentTemplateKey::Search
        );
    if forced_noindex && draft.seo.indexable {
        push(
            issues,
            CmsTemplateValidationCode::IndexingNotAllowed,
            "seo.indexable",
            "This placeholder or template is always excluded from indexing.",
        );
    }
    for (path, media) in rendered_media(draft) {
        if !media.decorative
            && media
                .alt_text
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            push(
                issues,
                CmsTemplateValidationCode::MissingMediaAlt,
                &path,
                "Non-decorative rendered media requires alternative text.",
            );
        }
    }
}

fn rendered_media(draft: &ContentDraftV2) -> Vec<(String, &MediaUseReference)> {
    let mut values = Vec::new();
    for (index, block) in draft.composition.blocks.iter().enumerate() {
        match block {
            ContentBlock::Hero(value) => {
                if let Some(media) = &value.media {
                    values.push((format!("composition.blocks.{index}.media"), media));
                }
            }
            ContentBlock::Media(value) => {
                values.push((format!("composition.blocks.{index}.media"), &value.media));
            }
            ContentBlock::FeatureGrid(value) => {
                for (item_index, item) in value.items.iter().enumerate() {
                    if let Some(media) = &item.icon {
                        values.push((
                            format!("composition.blocks.{index}.items.{item_index}.icon"),
                            media,
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    match &draft.type_fields {
        ContentTypeFields::Article(value) | ContentTypeFields::News(value) => {
            if let Some(media) = &value.cover {
                values.push(("typeFields.cover".into(), media));
            }
        }
        _ => {}
    }
    values
}

fn validate_relations(draft: &ContentDraftV2, issues: &mut Vec<CmsTemplateValidationIssue>) {
    let mut relation_ids = HashSet::new();
    for (index, relation) in draft.relations.iter().enumerate() {
        if !relation_ids.insert(relation.id) {
            push(
                issues,
                CmsTemplateValidationCode::DuplicateRelationId,
                &format!("relations.{index}.id"),
                "Relation IDs must be unique within a draft.",
            );
        }
    }
    for (block_index, block) in draft.composition.blocks.iter().enumerate() {
        let ContentBlock::RelationCollection(collection) = block else {
            continue;
        };
        for (relation_index, id) in collection.relation_ids.iter().enumerate() {
            if !relation_ids.contains(id) {
                push(
                    issues,
                    CmsTemplateValidationCode::MissingRelationReference,
                    &format!("composition.blocks.{block_index}.relationIds.{relation_index}"),
                    "The relation collection references an unknown draft relation ID.",
                );
            }
        }
    }
}

fn type_fields_kind(fields: &ContentTypeFields) -> CmsContentKind {
    match fields {
        ContentTypeFields::Home => CmsContentKind::Home,
        ContentTypeFields::Page => CmsContentKind::Page,
        ContentTypeFields::Solution(_) => CmsContentKind::Solution,
        ContentTypeFields::Technology(_) => CmsContentKind::Technology,
        ContentTypeFields::Article(_) => CmsContentKind::Article,
        ContentTypeFields::News(_) => CmsContentKind::News,
        ContentTypeFields::Faq(_) => CmsContentKind::Faq,
        ContentTypeFields::CaseStudy(_) => CmsContentKind::CaseStudy,
        ContentTypeFields::Download(_) => CmsContentKind::Download,
        ContentTypeFields::Company => CmsContentKind::Company,
        ContentTypeFields::Legal(_) => CmsContentKind::Legal,
        ContentTypeFields::GeneralInformation(_) => CmsContentKind::GeneralInformation,
        ContentTypeFields::Navigation(_) => CmsContentKind::Navigation,
        ContentTypeFields::Footer(_) => CmsContentKind::Footer,
    }
}

fn block_id(block: &ContentBlock) -> uuid::Uuid {
    match block {
        ContentBlock::Hero(value) => value.id,
        ContentBlock::Body(value) => value.id,
        ContentBlock::Media(value) => value.id,
        ContentBlock::FeatureGrid(value) => value.id,
        ContentBlock::Evidence(value) => value.id,
        ContentBlock::Cta(value) => value.id,
        ContentBlock::RelationCollection(value) => value.id,
        ContentBlock::FaqCollection(value) => value.id,
        ContentBlock::DownloadAsset(value) => value.id,
        ContentBlock::ContactBlock(value) => value.id,
    }
}

fn push(
    issues: &mut Vec<CmsTemplateValidationIssue>,
    code: CmsTemplateValidationCode,
    path: &str,
    message: &str,
) {
    issues.push(CmsTemplateValidationIssue {
        code,
        path: path.into(),
        message: message.into(),
    });
}

#[cfg(test)]
mod tests;
