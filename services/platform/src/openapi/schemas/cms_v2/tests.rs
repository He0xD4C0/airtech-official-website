use std::collections::BTreeSet;

use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::models::{
    AssetVersionReference, CmsContentKind, ContentBlock, ContentDraftV2, ContentRelationReference,
    ContentTemplateKey, ContentTypeFields, DownloadTypeFields, HeroBlock, HeroVariant,
    PageComposition, RelationTargetReference, SeoInput, CMS_V2_SCHEMA_VERSION,
};
use crate::services::cms_templates::template_registry;

fn schemas() -> Map<String, Value> {
    let mut schemas = Map::new();
    super::add(&mut schemas);
    schemas
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect()
}

fn assert_object_shape(schema: &Value, serialized: &Value) {
    assert_eq!(schema["additionalProperties"], false);
    let allowed = keys(&schema["properties"]);
    let actual = keys(serialized);
    assert!(actual.is_subset(&allowed));
    for required in schema["required"].as_array().expect("required") {
        assert!(actual.contains(required.as_str().expect("required name")));
    }
}

#[test]
fn representative_rust_serialization_matches_draft_block_and_relation_schemas() {
    let relation = ContentRelationReference {
        id: Uuid::from_u128(3),
        slot: "relatedProducts".into(),
        target: RelationTargetReference::Product {
            product_id: Uuid::from_u128(4),
        },
    };
    let draft = ContentDraftV2 {
        schema_version: CMS_V2_SCHEMA_VERSION,
        kind: CmsContentKind::Download,
        locale: "en".into(),
        template_key: ContentTemplateKey::DownloadDetail,
        title: "Download".into(),
        slug: Some("download".into()),
        summary: None,
        is_placeholder: false,
        type_fields: ContentTypeFields::Download(DownloadTypeFields {
            version_label: None,
            resource_type: None,
            version_notes: None,
        }),
        body: None,
        composition: PageComposition {
            blocks: vec![ContentBlock::Hero(HeroBlock {
                id: Uuid::from_u128(2),
                eyebrow: None,
                heading: Some("Download".into()),
                lead: None,
                media: None,
                actions: Vec::new(),
                variant: HeroVariant::Standard,
            })],
        },
        seo: SeoInput::default(),
        relations: vec![relation],
        draft_version: 1,
    };
    let encoded = serde_json::to_value(draft).expect("serialize V2 draft");
    let schemas = schemas();

    assert_object_shape(&schemas["ContentDraftV2"], &encoded);
    assert_object_shape(&schemas["HeroBlock"], &encoded["composition"]["blocks"][0]);
    assert_object_shape(
        &schemas["ContentRelationReference"],
        &encoded["relations"][0],
    );
    assert_eq!(
        encoded["typeFields"],
        json!({
            "type": "download",
            "versionLabel": null,
            "resourceType": null,
            "versionNotes": null
        })
    );
    assert_eq!(encoded["composition"]["blocks"][0]["type"], "hero");
}

#[test]
fn contract_uses_rust_discriminators_and_single_source_fields() {
    let schemas = schemas();
    assert_eq!(
        schemas["ContentBlock"]["discriminator"]["propertyName"],
        "type"
    );
    assert_eq!(
        schemas["ContentTypeFields"]["discriminator"]["propertyName"],
        "type"
    );
    assert_eq!(
        keys(&schemas["PageComposition"]["properties"]),
        BTreeSet::from(["blocks"])
    );
    assert!(schemas["SeoInputV2"]["properties"]
        .get("canonicalPath")
        .is_none());
    assert_eq!(
        schemas["SeoInputV2"]["properties"]["socialImage"]["anyOf"][0]["$ref"],
        "#/components/schemas/MediaUseReference"
    );
    assert_eq!(
        schemas["LinkTargetRoute"]["properties"]["path"]["pattern"],
        r"^/(?![/\\])[^?#\s\\]*$"
    );
    assert_eq!(
        schemas["GeneralInformationTypeFields"]["properties"]["homePath"]["anyOf"][0]["pattern"],
        r"^/(?![/\\])[^?#\s\\]*$"
    );
    assert!(schemas["ContentDraftV2"]["required"]
        .as_array()
        .expect("required")
        .contains(&json!("isPlaceholder")));
    assert!(schemas["ContentDraftV2"]["required"]
        .as_array()
        .expect("required")
        .contains(&json!("relations")));
    assert_eq!(
        keys(&schemas["DownloadTypeFields"]["properties"]),
        BTreeSet::from(["resourceType", "versionLabel", "versionNotes"])
    );
    assert!(schemas["DownloadTypeFields"]["properties"]
        .get("asset")
        .is_none());
    assert!(
        schemas["ContentDraftV2"]["properties"]["slug"]["anyOf"][0]["pattern"]
            .as_str()
            .expect("draft slug pattern")
            .starts_with("^(?:")
    );
}

#[test]
fn product_relation_target_cannot_copy_product_facts() {
    let schemas = schemas();
    let product = &schemas["RelationTargetProduct"];
    assert_eq!(product["additionalProperties"], false);
    assert_eq!(
        keys(&product["properties"]),
        BTreeSet::from(["productId", "targetType"])
    );
    let target = RelationTargetReference::Product {
        product_id: Uuid::from_u128(9),
    };
    assert_object_shape(
        product,
        &serde_json::to_value(target).expect("serialize target"),
    );
}

#[test]
fn templates_general_information_and_migration_report_are_exact() {
    let schemas = schemas();
    let templates = schemas["ContentTemplateKey"]["enum"]
        .as_array()
        .expect("template enum");
    assert!(!templates.contains(&json!("productDetail")));
    assert!(templates.contains(&json!("articleIndex")));
    assert!(templates.contains(&json!("articleDetail")));
    assert_eq!(
        schemas["CmsPublicationStatusV2"]["enum"],
        json!(["draft", "published", "archived"])
    );

    let general_information = schemas["ContentTypeFields"]["oneOf"]
        .as_array()
        .expect("type field variants")
        .iter()
        .find(|schema| schema["properties"]["type"]["const"] == "generalInformation")
        .expect("general information variant");
    assert!(general_information["properties"]
        .get("productCategories")
        .is_some());
    assert!(general_information["properties"]
        .get("navigationCta")
        .is_some());

    assert_eq!(
        keys(&schemas["MigrationPreflightReport"]["properties"]),
        BTreeSet::from([
            "blockingIssueCount",
            "canMigrate",
            "convertible",
            "generatedAt",
            "issues",
            "scanned",
            "targetSchemaVersion",
            "warningCount",
        ])
    );
    assert_eq!(
        schemas["MigrationPreflightSource"]["enum"],
        json!([
            "content",
            "contentRevision",
            "news",
            "generalInformation",
            "media",
            "relation",
            "route"
        ])
    );
    assert_eq!(
        schemas["MigrationPreflightIssueCode"]["enum"],
        json!([
            "invalidLegacyPayload",
            "unsupportedContentKind",
            "unsupportedTemplate",
            "embeddedPageSlots",
            "unknownBlock",
            "invalidTiptapDocument",
            "typeFieldMismatch",
            "invalidRelation",
            "missingRelationTarget",
            "relationHistoryUnavailable",
            "missingMediaAsset",
            "missingMediaVersion",
            "duplicatePublicPath",
            "missingPublicRoute",
            "routeOwnershipMismatch",
            "canonicalPathMismatch",
            "scheduledPublicationUnsupported",
            "revisionConversionFailed"
        ])
    );
    assert_eq!(
        keys(&schemas["MigrationPreflightCounts"]["properties"]),
        BTreeSet::from([
            "contentEntries",
            "contentRevisions",
            "generalInformationEntries",
            "mediaAssets",
            "mediaReferences",
            "newsEntries",
            "publicRoutes",
            "relations",
        ])
    );
}

#[test]
fn every_declared_object_schema_rejects_unknown_properties() {
    fn walk(value: &Value) {
        if value["type"] == "object" {
            assert_eq!(value["additionalProperties"], false, "{value}");
        }
        match value {
            Value::Array(values) => values.iter().for_each(walk),
            Value::Object(values) => values.values().for_each(walk),
            _ => {}
        }
    }
    schemas().values().for_each(walk);
}

#[test]
fn media_assets_are_always_explicit() {
    let reference = AssetVersionReference {
        asset_id: Uuid::from_u128(20),
    };
    assert_object_shape(
        &schemas()["AssetVersionReference"],
        &serde_json::to_value(reference).expect("serialize asset version"),
    );
}

#[test]
fn openapi_template_keys_equal_the_runtime_registry() {
    let schemas = schemas();
    let documented = schemas["ContentTemplateKey"]["enum"]
        .as_array()
        .expect("template enum")
        .iter()
        .map(|value| value.as_str().expect("template string").to_owned())
        .collect::<BTreeSet<_>>();
    let registered = template_registry()
        .iter()
        .map(|definition| {
            serde_json::to_value(definition.key)
                .expect("serialize template key")
                .as_str()
                .expect("template string")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(documented, registered);
}
