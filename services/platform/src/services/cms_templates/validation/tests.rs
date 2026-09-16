use serde_json::json;
use uuid::Uuid;

use crate::models::{
    AssetVersionReference, CmsContentKind, CollectionPresentation, ContactBlock,
    ContactChannelKind, ContentBlock, ContentDraftV2, ContentTemplateKey, ContentTypeFields,
    EditorialAction, FaqCollectionBlock, FaqItem, FaqTypeFields, HeroBlock, HeroVariant,
    LinkTargetReference, MediaUseReference, PageComposition, RelationCollectionBlock, SeoInput,
    TiptapDocument, TiptapRootType, CMS_V2_SCHEMA_VERSION,
};

use super::{validate_content_draft, CmsTemplateValidationCode, CmsTemplateValidationPhase};

fn home_draft() -> ContentDraftV2 {
    ContentDraftV2 {
        schema_version: CMS_V2_SCHEMA_VERSION,
        kind: CmsContentKind::Home,
        locale: "en".into(),
        template_key: ContentTemplateKey::Home,
        title: "Home".into(),
        slug: Some("index".into()),
        summary: None,
        is_placeholder: false,
        type_fields: ContentTypeFields::Home,
        body: None,
        composition: PageComposition {
            blocks: vec![ContentBlock::Hero(HeroBlock {
                id: Uuid::from_u128(1),
                eyebrow: None,
                heading: Some("AIRTEKPOWER".into()),
                lead: None,
                media: None,
                actions: Vec::new(),
                variant: HeroVariant::Standard,
            })],
        },
        seo: SeoInput::default(),
        relations: Vec::new(),
        draft_version: 1,
    }
}

#[test]
fn accepts_a_coherent_registered_draft() {
    assert!(validate_content_draft(&home_draft(), CmsTemplateValidationPhase::Publish).is_empty());
}

#[test]
fn structural_validation_allows_an_incomplete_working_draft() {
    let mut draft = home_draft();
    draft.title.clear();
    draft.slug = None;
    draft.composition.blocks.clear();
    assert!(validate_content_draft(&draft, CmsTemplateValidationPhase::Structural).is_empty());
}

#[test]
fn rejects_schema_kind_and_type_field_drift() {
    let mut draft = home_draft();
    draft.schema_version = 1;
    draft.kind = CmsContentKind::Page;
    let codes = validate_content_draft(&draft, CmsTemplateValidationPhase::Structural)
        .into_iter()
        .map(|issue| issue.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&CmsTemplateValidationCode::UnsupportedSchemaVersion));
    assert!(codes.contains(&CmsTemplateValidationCode::TemplateKindMismatch));
    assert!(codes.contains(&CmsTemplateValidationCode::TypeFieldsKindMismatch));
}

#[test]
fn requires_body_content_to_have_a_composition_position() {
    let mut draft = home_draft();
    draft.body = Some(TiptapDocument {
        node_type: TiptapRootType::Doc,
        content: vec![json!({"type": "paragraph"})],
    });
    assert!(
        validate_content_draft(&draft, CmsTemplateValidationPhase::Publish)
            .iter()
            .any(|issue| issue.code == CmsTemplateValidationCode::UnplacedBody)
    );
}

#[test]
fn placeholder_and_fixed_noindex_templates_cannot_be_indexable() {
    let mut draft = home_draft();
    draft.is_placeholder = true;
    draft.seo.indexable = true;
    assert!(
        validate_content_draft(&draft, CmsTemplateValidationPhase::Publish)
            .iter()
            .any(|issue| issue.code == CmsTemplateValidationCode::IndexingNotAllowed)
    );
}

#[test]
fn non_decorative_rendered_media_requires_alt_text_at_publish() {
    let mut draft = home_draft();
    let ContentBlock::Hero(hero) = &mut draft.composition.blocks[0] else {
        unreachable!();
    };
    hero.media = Some(MediaUseReference {
        asset: AssetVersionReference {
            asset_id: Uuid::from_u128(2),
        },
        alt_text: None,
        decorative: false,
    });
    assert!(
        validate_content_draft(&draft, CmsTemplateValidationPhase::Publish)
            .iter()
            .any(|issue| issue.code == CmsTemplateValidationCode::MissingMediaAlt)
    );
}

#[test]
fn validates_rich_text_nested_in_faq_items() {
    let mut draft = home_draft();
    draft.kind = CmsContentKind::Faq;
    draft.template_key = ContentTemplateKey::FaqDetail;
    draft.type_fields = ContentTypeFields::Faq(FaqTypeFields {
        items: vec![FaqItem {
            id: Uuid::from_u128(4),
            question: "Question".into(),
            answer: TiptapDocument {
                node_type: TiptapRootType::Doc,
                content: vec![json!({"type": "iframe"})],
            },
        }],
    });
    draft
        .composition
        .blocks
        .push(ContentBlock::FaqCollection(FaqCollectionBlock {
            id: Uuid::from_u128(5),
            heading: None,
        }));
    assert!(
        validate_content_draft(&draft, CmsTemplateValidationPhase::Structural)
            .iter()
            .any(|issue| issue.code == CmsTemplateValidationCode::InvalidRichText)
    );
}

#[test]
fn rejects_unsafe_external_action_targets() {
    let mut draft = home_draft();
    let ContentBlock::Hero(hero) = &mut draft.composition.blocks[0] else {
        unreachable!();
    };
    hero.actions.push(EditorialAction {
        label: "Unsafe".into(),
        target: LinkTargetReference::External {
            url: "javascript:alert(1)".into(),
        },
    });
    assert!(
        validate_content_draft(&draft, CmsTemplateValidationPhase::Structural)
            .iter()
            .any(|issue| issue.code == CmsTemplateValidationCode::InvalidLink)
    );
}

#[test]
fn structural_validation_rejects_invalid_locale_contract_values() {
    for locale in ["e", "en_US", "engl", "en-", "en-123456789"] {
        let mut draft = home_draft();
        draft.locale = locale.into();
        assert!(
            validate_content_draft(&draft, CmsTemplateValidationPhase::Structural)
                .iter()
                .any(|issue| {
                    issue.code == CmsTemplateValidationCode::InvalidContractValue
                        && issue.path == "locale"
                }),
            "locale {locale:?} should be rejected"
        );
    }
}

#[test]
fn structural_validation_rejects_invalid_slug_contract_values() {
    for slug in ["Bad-slug", "bad_slug", "-bad", "bad-", "two--parts"] {
        let mut draft = home_draft();
        draft.slug = Some(slug.into());
        assert!(
            validate_content_draft(&draft, CmsTemplateValidationPhase::Structural)
                .iter()
                .any(|issue| {
                    issue.code == CmsTemplateValidationCode::InvalidContractValue
                        && issue.path == "slug"
                }),
            "slug {slug:?} should be rejected"
        );
    }
}

#[test]
fn structural_validation_enforces_nested_string_and_collection_limits() {
    let mut draft = home_draft();
    draft.title = "t".repeat(301);
    draft.summary = Some("s".repeat(2_001));
    draft.seo.description = Some("d".repeat(1_001));
    let ContentBlock::Hero(hero) = &mut draft.composition.blocks[0] else {
        unreachable!();
    };
    hero.heading = Some("h".repeat(301));
    hero.media = Some(MediaUseReference {
        asset: AssetVersionReference {
            asset_id: Uuid::from_u128(20),
        },
        alt_text: Some("a".repeat(501)),
        decorative: false,
    });
    hero.actions = (0..3)
        .map(|index| EditorialAction {
            label: if index == 0 {
                "l".repeat(121)
            } else {
                "Action".into()
            },
            target: LinkTargetReference::Route {
                path: format!("/en/action-{index}"),
            },
        })
        .collect();

    let issues = validate_content_draft(&draft, CmsTemplateValidationPhase::Structural);
    for path in [
        "title",
        "summary",
        "seo.description",
        "composition.blocks.0.heading",
        "composition.blocks.0.media.altText",
        "composition.blocks.0.actions",
        "composition.blocks.0.actions.0.label",
    ] {
        assert!(
            issues.iter().any(|issue| {
                issue.code == CmsTemplateValidationCode::InvalidContractValue && issue.path == path
            }),
            "expected an invalid contract value at {path}"
        );
    }
}

#[test]
fn structural_validation_rejects_duplicate_relation_ids_and_contact_channels() {
    let mut draft = home_draft();
    let relation_id = Uuid::from_u128(30);
    draft
        .composition
        .blocks
        .push(ContentBlock::RelationCollection(RelationCollectionBlock {
            id: Uuid::from_u128(31),
            heading: None,
            relation_ids: vec![relation_id, relation_id],
            presentation: CollectionPresentation::Cards,
        }));
    draft
        .composition
        .blocks
        .push(ContentBlock::ContactBlock(ContactBlock {
            id: Uuid::from_u128(32),
            heading: None,
            channels: vec![ContactChannelKind::Email, ContactChannelKind::Email],
            action: None,
        }));

    let issues = validate_content_draft(&draft, CmsTemplateValidationPhase::Structural);
    for path in [
        "composition.blocks.1.relationIds",
        "composition.blocks.2.channels",
    ] {
        assert!(
            issues.iter().any(|issue| {
                issue.code == CmsTemplateValidationCode::InvalidContractValue && issue.path == path
            }),
            "expected duplicate detection at {path}"
        );
    }
}
