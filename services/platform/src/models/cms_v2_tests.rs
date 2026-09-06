use super::*;
use serde_json::json;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn sample_draft() -> ContentDraftV2 {
    let product_relation_id = id(4);
    ContentDraftV2 {
        schema_version: CMS_V2_SCHEMA_VERSION,
        kind: CmsContentKind::News,
        locale: "en".into(),
        template_key: ContentTemplateKey::NewsDetail,
        title: "Migration contract".into(),
        slug: Some("migration-contract".into()),
        summary: Some("A strict V2 contract fixture.".into()),
        is_placeholder: false,
        type_fields: ContentTypeFields::News(EditorialTypeFields {
            category: Some("engineering".into()),
            author_display_name: None,
            publication_at: None,
            cover: None,
            featured: false,
        }),
        body: Some(TiptapDocument {
            node_type: TiptapRootType::Doc,
            content: vec![json!({
                "type": "paragraph",
                "content": [{"type": "text", "text": "Strict body"}]
            })],
        }),
        composition: PageComposition {
            blocks: vec![
                ContentBlock::Hero(HeroBlock {
                    id: id(1),
                    eyebrow: None,
                    heading: Some("Migration contract".into()),
                    lead: None,
                    media: None,
                    actions: Vec::new(),
                    variant: HeroVariant::Standard,
                }),
                ContentBlock::Body(BodyBlock {
                    id: id(2),
                    width: ContentWidth::Standard,
                }),
                ContentBlock::RelationCollection(RelationCollectionBlock {
                    id: id(3),
                    heading: Some("Related products".into()),
                    relation_ids: vec![product_relation_id],
                    presentation: CollectionPresentation::Cards,
                }),
            ],
        },
        seo: SeoInput {
            title: Some("Migration contract".into()),
            description: None,
            indexable: false,
            social_image: None,
        },
        relations: vec![ContentRelationReference {
            id: product_relation_id,
            slot: "relatedProducts".into(),
            target: RelationTargetReference::Product { product_id: id(5) },
        }],
        draft_version: 1,
    }
}

#[test]
fn content_draft_v2_round_trips_with_camel_case_discriminators() {
    let draft = sample_draft();
    let encoded = serde_json::to_value(&draft).expect("serialize V2 draft");

    assert_eq!(encoded["schemaVersion"], json!(2));
    assert_eq!(encoded["templateKey"], json!("newsDetail"));
    assert_eq!(encoded["isPlaceholder"], json!(false));
    assert_eq!(encoded["typeFields"]["type"], json!("news"));
    assert_eq!(encoded["composition"]["blocks"][0]["type"], json!("hero"));
    assert!(encoded.get("schema_version").is_none());

    let decoded: ContentDraftV2 = serde_json::from_value(encoded).expect("deserialize V2 draft");
    assert_eq!(decoded, draft);
}

#[test]
fn strict_contract_rejects_unknown_fields_and_legacy_page_slots() {
    let mut top_level = serde_json::to_value(sample_draft()).expect("serialize fixture");
    top_level["legacyField"] = json!(true);
    assert!(serde_json::from_value::<ContentDraftV2>(top_level).is_err());

    let mut block = serde_json::to_value(sample_draft()).expect("serialize fixture");
    block["composition"]["blocks"][0]["legacyStyle"] = json!("freeform");
    assert!(serde_json::from_value::<ContentDraftV2>(block).is_err());

    let mut body = serde_json::to_value(sample_draft()).expect("serialize fixture");
    body["body"]["attrs"] = json!({"pageSlots": {"templateKey": "legacy"}});
    assert!(serde_json::from_value::<ContentDraftV2>(body).is_err());
}

#[test]
fn product_relations_store_only_the_stable_product_uuid() {
    let product_id = id(9);
    let target = RelationTargetReference::Product { product_id };
    let encoded = serde_json::to_value(&target).expect("serialize product relation");

    assert_eq!(
        encoded,
        json!({"targetType": "product", "productId": product_id})
    );
    assert!(encoded.get("model").is_none());
    assert!(encoded.get("specifications").is_none());

    let copied_fact = json!({
        "targetType": "product",
        "productId": product_id,
        "model": "unvalidated-copy"
    });
    assert!(serde_json::from_value::<RelationTargetReference>(copied_fact).is_err());
}

#[test]
fn v2_publication_status_does_not_retain_scheduling() {
    assert_eq!(
        serde_json::to_value(CmsPublicationStatusV2::Published).unwrap(),
        json!("published")
    );
    assert!(serde_json::from_value::<CmsPublicationStatusV2>(json!("scheduled")).is_err());
}
