#![allow(dead_code, unused_imports)]

use std::collections::BTreeSet;

use serde_json::{json, Value};
use uuid::Uuid;

mod models {
    pub use airtek_platform::models::{ContentDraftV2, CMS_V2_SCHEMA_VERSION};
}

#[path = "../src/services/cms_publication_dependencies.rs"]
mod dependencies;

use dependencies::{
    extract_document, DependencyIssueCode, PublicationDependencyKind, PublicationLockTargetKind,
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn empty_document() -> Value {
    json!({
        "schemaVersion": 2,
        "locale": "en",
        "typeFields": {"type": "page"},
        "seo": {"socialImage": null},
        "composition": {"blocks": []},
        "relations": []
    })
}

#[test]
fn extractor_emits_stable_json_pointers_and_sorted_lock_targets() {
    let content_id = id(10);
    let product_id = id(20);
    let asset_id = id(30);
    let document = json!({
        "schemaVersion": 2,
        "locale": "en",
        "typeFields": {
            "type": "news",
            "cover": {"asset": {"assetId": asset_id}}
        },
        "seo": {
            "socialImage": {"asset": {"assetId": asset_id}}
        },
        "composition": {"blocks": [
            {
                "type": "hero",
                "media": {"asset": {"assetId": asset_id}},
                "actions": [{
                    "target": {"targetType": "content", "contentId": content_id}
                }]
            },
            {
                "type": "featureGrid",
                "items": [{
                    "icon": {"asset": {"assetId": asset_id}}
                }]
            },
            {
                "type": "downloadAsset",
                "asset": {"assetId": asset_id}
            }
        ]},
        "relations": [
            {"id": id(40), "target": {"targetType": "content", "contentId": content_id}},
            {"id": id(41), "target": {"targetType": "product", "productId": product_id}}
        ]
    });

    let extracted = extract_document(&document);
    assert!(extracted.blocking_issues.is_empty());
    let actual = extracted
        .references
        .iter()
        .map(|dependency| (dependency.reference_path.as_str(), dependency.kind))
        .collect::<BTreeSet<_>>();
    let expected = BTreeSet::from([
        (
            "/composition/blocks/0/actions/0/target/contentId",
            PublicationDependencyKind::ContentLink,
        ),
        (
            "/composition/blocks/0/media/asset/assetId",
            PublicationDependencyKind::MediaInline,
        ),
        (
            "/composition/blocks/1/items/0/icon/asset/assetId",
            PublicationDependencyKind::MediaInline,
        ),
        (
            "/composition/blocks/2/asset/assetId",
            PublicationDependencyKind::MediaDownload,
        ),
        (
            "/relations/0/target/contentId",
            PublicationDependencyKind::RelationContent,
        ),
        (
            "/relations/1/target/productId",
            PublicationDependencyKind::RelationProduct,
        ),
        (
            "/seo/socialImage/asset/assetId",
            PublicationDependencyKind::MediaInline,
        ),
        (
            "/typeFields/cover/asset/assetId",
            PublicationDependencyKind::MediaInline,
        ),
    ]);
    assert_eq!(actual, expected);
    assert_eq!(extracted.lock_targets.len(), 3);
    assert_eq!(
        extracted.lock_targets[0].kind,
        PublicationLockTargetKind::Content
    );
    assert_eq!(extracted.lock_targets[0].id, content_id);
    assert_eq!(
        extracted.lock_targets[1].kind,
        PublicationLockTargetKind::Product
    );
    assert_eq!(extracted.lock_targets[1].id, product_id);
    assert_eq!(
        extracted.lock_targets[2].kind,
        PublicationLockTargetKind::Media
    );
    assert_eq!(extracted.lock_targets[2].id, asset_id);
}

#[test]
fn malformed_dependency_fields_become_stable_blocking_issues() {
    let mut document = empty_document();
    document["relations"] = json!([
        {"id": 7, "target": {"targetType": "content", "contentId": "not-a-uuid"}},
        {"id": id(2), "target": {"targetType": 9}}
    ]);
    document["composition"]["blocks"] = json!([{
        "type": "media",
        "media": {"asset": {"assetId": 12}}
    }]);

    let extracted = extract_document(&document);
    let issues = extracted
        .blocking_issues
        .iter()
        .map(|issue| (issue.path.as_str(), issue.code))
        .collect::<BTreeSet<_>>();
    assert!(issues.contains(&("/relations/0/id", DependencyIssueCode::InvalidType)));
    assert!(issues.contains(&(
        "/relations/0/target/contentId",
        DependencyIssueCode::InvalidUuid
    )));
    assert!(issues.contains(&(
        "/relations/1/target/targetType",
        DependencyIssueCode::InvalidType
    )));
    assert!(issues.contains(&(
        "/composition/blocks/0/media/asset/assetId",
        DependencyIssueCode::InvalidType
    )));
    assert!(extracted.references.is_empty());
}

#[test]
fn nested_navigation_links_use_array_index_json_pointers() {
    let content_id = id(99);
    let mut document = empty_document();
    document["typeFields"] = json!({
        "type": "footer",
        "columns": [{"links": [{
            "target": null,
            "children": [{
                "target": {"targetType": "content", "contentId": content_id},
                "children": []
            }]
        }]}],
        "legalLinks": []
    });

    let extracted = extract_document(&document);
    assert!(extracted.blocking_issues.is_empty());
    assert_eq!(extracted.references.len(), 1);
    assert_eq!(
        extracted.references[0].reference_path,
        "/typeFields/columns/0/links/0/children/0/target/contentId"
    );
}

#[test]
fn general_information_site_icon_is_an_exact_media_dependency() {
    let asset_id = id(123);
    let mut document = empty_document();
    document["typeFields"] = json!({
        "type": "generalInformation",
        "siteIcon": {"assetId": asset_id},
        "navigationCta": null,
        "defaultSeo": {"socialImage": null}
    });

    let extracted = extract_document(&document);
    assert!(extracted.blocking_issues.is_empty());
    assert_eq!(extracted.references.len(), 1);
    assert_eq!(
        extracted.references[0].reference_path,
        "/typeFields/siteIcon/assetId"
    );
    assert_eq!(
        extracted.references[0].kind,
        PublicationDependencyKind::MediaInline
    );
    assert_eq!(extracted.lock_targets.len(), 1);
    assert_eq!(
        extracted.lock_targets[0].kind,
        PublicationLockTargetKind::Media
    );
    assert_eq!(extracted.lock_targets[0].id, asset_id);
}
