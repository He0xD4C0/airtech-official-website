use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::models::{
    CmsContentKind, ContentBlock, ContentTypeFields, MigrationPreflightIssueCode,
    MigrationPreflightSeverity, RelationTargetReference,
};

use super::{
    analyze_legacy_snapshot,
    conversion::{convert_content, ContentSource},
    general_information::{convert_general_information, GeneralInformationSource},
    news::news_working_fields,
    references::stable_media_version_id,
    types::*,
};

pub(super) fn timestamp() -> DateTime<Utc> {
    "2026-09-04T00:00:00Z".parse().expect("timestamp")
}

pub(super) fn content_payload(template: &str, extra_attrs: Value, extra_slots: Value) -> Value {
    let mut slots = json!({
        "templateKey": template,
        "hero": {"eyebrow": "Evidence", "title": "Title", "description": "Lead"}
    });
    slots
        .as_object_mut()
        .expect("slots")
        .extend(extra_slots.as_object().cloned().unwrap_or_default());
    let mut attrs = Map::from_iter([("pageSlots".into(), slots)]);
    attrs.extend(extra_attrs.as_object().cloned().unwrap_or_default());
    json!({
        "summary": "Summary",
        "body": {
            "schemaVersion": 1,
            "doc": {
                "type": "doc",
                "attrs": attrs,
                "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Body"}]}]
            }
        },
        "seo": {"title": "SEO", "description": "Description", "indexable": false}
    })
}

pub(super) fn source(kind: &str, slug: &str, payload: Value) -> ContentSource {
    ContentSource {
        entity_id: Uuid::from_u128(1),
        revision: 1,
        role: CmsPreflightRecordRole::Working,
        kind: kind.into(),
        slug: slug.into(),
        locale: "en".into(),
        title: "Title".into(),
        is_published_revision: false,
        is_placeholder: false,
        allows_lossy_placeholder_cleanup: false,
        scheduled: false,
        payload,
        current_relations: Vec::new(),
        relation_history_unavailable: false,
    }
}

#[test]
fn extracts_page_slots_and_maps_article_attributes() {
    let payload = content_payload(
        "articleDetail",
        json!({
            "author": "A. Editor",
            "authorType": "Person",
            "publishedAt": "2026-09-04",
            "category": "Engineering"
        }),
        json!({}),
    );
    let (record, issues) = convert_content(source("article", "article", payload), Map::new());
    let draft = record.expect("lossless article").candidate;
    assert_eq!(draft.kind, CmsContentKind::Article);
    let ContentTypeFields::Article(fields) = draft.type_fields else {
        panic!("article fields");
    };
    assert_eq!(fields.author_display_name.as_deref(), Some("A. Editor"));
    assert_eq!(fields.category.as_deref(), Some("Engineering"));
    assert_eq!(fields.publication_at, Some(timestamp()));
    assert!(matches!(draft.composition.blocks[0], ContentBlock::Hero(_)));
    assert!(draft
        .composition
        .blocks
        .iter()
        .any(|block| matches!(block, ContentBlock::Body(_))));
    assert!(serde_json::to_value(&draft.body)
        .expect("body JSON")
        .pointer("/attrs/pageSlots")
        .is_none());
    assert!(issues
        .iter()
        .any(|issue| issue.code == MigrationPreflightIssueCode::EmbeddedPageSlots));
}

#[test]
fn maps_download_metadata_asset_description_and_product_ids() {
    let asset_id = Uuid::from_u128(20);
    let product_id = Uuid::from_u128(30);
    let payload = content_payload(
        "downloadDetail",
        json!({
            "version": "Revision 3",
            "resourceType": "Datasheet",
            "fileDescription": "Controlled file",
            "downloadUrl": "/media/file.pdf",
            "fileStatus": {"scan": "clean", "access": "public"},
            "applicableModels": [product_id]
        }),
        json!({}),
    );
    let seed = json!({
        "asset": {"assetId": asset_id, "versionId": stable_media_version_id(asset_id)},
        "assetScanStatus": "clean",
        "assetAccessLevel": "public",
        "productIds": [product_id]
    })
    .as_object()
    .cloned()
    .expect("seed");
    let (record, issues) = convert_content(source("download", "datasheet", payload), seed);
    let draft = record.expect("lossless download").candidate;
    let ContentTypeFields::Download(fields) = draft.type_fields else {
        panic!("download fields");
    };
    assert_eq!(fields.version_label.as_deref(), Some("Revision 3"));
    assert_eq!(fields.resource_type.as_deref(), Some("Datasheet"));
    let asset = draft
        .composition
        .blocks
        .iter()
        .find_map(|block| match block {
            ContentBlock::DownloadAsset(value) => Some(value),
            _ => None,
        });
    assert_eq!(
        asset.and_then(|value| value.description.as_deref()),
        Some("Controlled file")
    );
    assert!(draft
        .relations
        .iter()
        .any(|relation| { relation.target == RelationTargetReference::Product { product_id } }));
    assert!(issues
        .iter()
        .all(|issue| { issue.severity != MigrationPreflightSeverity::Blocking }));
}

#[test]
fn maps_news_and_general_information_metadata() {
    let media_id = Uuid::from_u128(40);
    let entry = legacy_entry(Uuid::from_u128(4), "news", "draft", None);
    let metadata = LegacyNewsWorking {
        content_id: entry.id,
        content_kind: "news".into(),
        category: "Company".into(),
        author_display_name: Some("Editor".into()),
        cover_media_asset_id: Some(media_id),
        featured: true,
        publication_at: Some(timestamp()),
        reading_minutes: Some(3),
        data_origin: entry.data_origin.clone(),
        updated_at: timestamp(),
    };
    let mut news_issues = Vec::new();
    let seed = news_working_fields(
        &entry,
        &metadata,
        &BTreeSet::from([media_id]),
        &mut news_issues,
    );
    let payload = content_payload("newsDetail", json!({}), json!({}));
    let (record, _) = convert_content(source("news", "news", payload), seed);
    let ContentTypeFields::News(news) = record.expect("news").candidate.type_fields else {
        panic!("news fields");
    };
    assert_eq!(news.category.as_deref(), Some("Company"));
    assert_eq!(news.cover.expect("cover").asset.asset_id, media_id);
    assert!(news_issues
        .iter()
        .any(|issue| { issue.message.contains("derives reading time") }));

    let gi = GeneralInformationSource {
        id: Uuid::from_u128(5),
        revision: 2,
        role: CmsPreflightRecordRole::Working,
        scope: "site".into(),
        locale: "en".into(),
        is_published_revision: false,
        is_placeholder: false,
        scheduled: false,
        payload: json!({
            "brandName": "AIRTEKPOWER",
            "brandLine": "Brand line",
            "homePath": "/en",
            "copyrightText": "Copyright {year}",
            "organization": {
                "name": "AIRTEKPOWER",
                "salesEmail": "sales@example.com",
                "address": "Line 1\nLine 2",
                "socialLinks": [{"label": "LinkedIn", "url": "https://example.com/company"}]
            },
            "defaultSeo": {"title": "Default", "description": "Description", "indexable": false},
            "navigationCta": {"label": "Contact", "href": "/en/contact"},
            "productCategories": []
        }),
    };
    let (record, issues) = convert_general_information(gi);
    let ContentTypeFields::GeneralInformation(fields) =
        record.expect("general information").candidate.type_fields
    else {
        panic!("GI fields");
    };
    assert_eq!(fields.organization_name.as_deref(), Some("AIRTEKPOWER"));
    assert_eq!(fields.contact.email.as_deref(), Some("sales@example.com"));
    assert_eq!(fields.contact.address_lines, ["Line 1", "Line 2"]);
    assert_eq!(
        fields.copyright_template.as_deref(),
        Some("Copyright {year}")
    );
    assert!(fields.navigation_cta.is_some());
    assert!(issues
        .iter()
        .all(|issue| issue.severity != MigrationPreflightSeverity::Blocking));
}

#[test]
fn rejects_unknown_blocks_references_and_unbound_download_description() {
    let payload = content_payload(
        "articleDetail",
        json!({}),
        json!({
            "mystery": {"value": true},
            "relationships": [{"entityType": "content", "path": "/not-an-id"}]
        }),
    );
    let (record, issues) = convert_content(source("article", "article", payload), Map::new());
    assert!(record.is_none());
    assert!(issues
        .iter()
        .any(|issue| issue.code == MigrationPreflightIssueCode::UnknownBlock));
    assert!(issues
        .iter()
        .any(|issue| issue.code == MigrationPreflightIssueCode::MissingRelationTarget));

    let download = content_payload(
        "downloadDetail",
        json!({"fileDescription": "Would be lost"}),
        json!({}),
    );
    let (record, issues) = convert_content(source("download", "file", download), Map::new());
    assert!(record.is_none());
    assert!(issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::MissingMediaAsset
            && issue.json_path.as_deref() == Some("body.doc.attrs.fileDescription")
    }));

    let mut historical = source(
        "article",
        "historical",
        content_payload("articleDetail", json!({}), json!({})),
    );
    historical.role = CmsPreflightRecordRole::Revision;
    historical.relation_history_unavailable = true;
    let (_, issues) = convert_content(historical, Map::new());
    assert!(issues
        .iter()
        .any(|issue| { issue.code == MigrationPreflightIssueCode::RelationHistoryUnavailable }));
}

#[test]
fn candidate_integrity_blocks_missing_relation_and_media() {
    let missing_content = Uuid::from_u128(700);
    let missing_media = Uuid::from_u128(701);
    let mut entry = legacy_entry(Uuid::from_u128(702), "home", "draft", None);
    let slots = entry
        .payload
        .pointer_mut("/body/doc/attrs/pageSlots")
        .and_then(Value::as_object_mut)
        .expect("page slots");
    slots.insert(
        "relationships".into(),
        json!([{"entityType": "content", "entityId": missing_content}]),
    );
    slots.insert(
        "media".into(),
        json!({
            "media": {
                "asset": {"assetId": missing_media, "versionId": stable_media_version_id(missing_media)},
                "altText": "Diagram",
                "decorative": false
            },
            "caption": null,
            "layout": "inline"
        }),
    );
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![entry],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert!(report
        .issues
        .iter()
        .any(|issue| { issue.code == MigrationPreflightIssueCode::MissingRelationTarget }));
    assert!(report
        .issues
        .iter()
        .any(|issue| { issue.code == MigrationPreflightIssueCode::MissingMediaAsset }));
}

#[test]
fn blocks_both_scheduled_signals_and_non_object_revision_payloads() {
    let first = legacy_entry(Uuid::from_u128(10), "home", "scheduled", None);
    let second = legacy_entry(Uuid::from_u128(11), "home", "draft", Some(timestamp()));
    let malformed = LegacyContentRevision {
        content_id: first.id,
        revision: 1,
        payload: json!("not-an-object"),
        created_by: "tester".into(),
        created_at: timestamp(),
    };
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![first, second],
            content_revisions: vec![malformed],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert_eq!(
        report
            .issues
            .iter()
            .filter(|issue| {
                issue.code == MigrationPreflightIssueCode::ScheduledPublicationUnsupported
            })
            .count(),
        2
    );
    assert!(report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::InvalidLegacyPayload
            && issue.source == CmsPreflightSource::ContentRevision
            && issue.json_path.as_deref() == Some("payload")
    }));
}

#[test]
fn report_is_deterministic_for_reordered_snapshot_rows() {
    let first = legacy_entry(Uuid::from_u128(100), "home", "draft", None);
    let mut second = legacy_entry(Uuid::from_u128(200), "home", "draft", None);
    second.locale = "fr".into();
    second
        .payload
        .as_object_mut()
        .expect("payload")
        .insert("locale".into(), json!("fr"));
    let relation = LegacyContentRelation {
        from_type: "content".into(),
        from_id: first.id,
        relation_type: "related".into(),
        to_type: "content".into(),
        to_id: second.id,
        sort_order: 0,
    };
    let left = LegacySnapshot {
        content_entries: vec![first.clone(), second.clone()],
        content_relations: vec![relation.clone()],
        ..LegacySnapshot::default()
    };
    let right = LegacySnapshot {
        content_entries: vec![second, first],
        content_relations: vec![relation],
        ..LegacySnapshot::default()
    };
    let left = analyze_legacy_snapshot(left, timestamp());
    let right = analyze_legacy_snapshot(right, timestamp());
    assert!(left.can_migrate);
    assert_eq!(left.scanned, left.convertible);
    assert_eq!(left, right);
}

pub(super) fn legacy_entry(
    id: Uuid,
    kind: &str,
    status: &str,
    scheduled_for: Option<DateTime<Utc>>,
) -> LegacyContentEntry {
    let template = if kind == "news" {
        "newsDetail"
    } else {
        "productIndex"
    };
    let slug = format!("entry-{}", id.as_u128());
    let title = "Title".to_owned();
    let mut payload = content_payload(template, json!({}), json!({}));
    payload
        .as_object_mut()
        .expect("payload")
        .extend(Map::from_iter([
            ("id".into(), json!(id)),
            ("kind".into(), json!(kind)),
            ("slug".into(), json!(slug)),
            ("locale".into(), json!("en")),
            ("title".into(), json!(title)),
            ("status".into(), json!(status)),
            ("isPlaceholder".into(), json!(false)),
            ("currentRevision".into(), json!(1)),
            ("publishedRevision".into(), Value::Null),
            ("scheduledFor".into(), json!(scheduled_for)),
            ("updatedAt".into(), json!(timestamp())),
        ]));
    LegacyContentEntry {
        id,
        kind: kind.into(),
        slug,
        locale: "en".into(),
        title,
        status: status.into(),
        is_placeholder: false,
        data_origin: "editorial".into(),
        current_revision: 1,
        published_revision: None,
        scheduled_for,
        payload,
        updated_at: timestamp(),
    }
}
