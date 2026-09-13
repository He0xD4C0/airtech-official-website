use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::{
    ContentBlock, LinkTargetReference, MigrationPreflightIssueCode, MigrationPreflightSeverity,
};

use super::{
    analyze_legacy_snapshot,
    conversion::{convert_content, ContentSource},
    integrity::validate_candidate_targets,
    tests::{legacy_entry, timestamp},
    types::{
        CmsPreflightRecordRole, LegacyAssetReference, LegacyContentEntry, LegacyContentRevision,
        LegacyMediaAsset, LegacyNews, LegacyNewsWorking, LegacySnapshot,
    },
};

fn asset(id: Uuid, deleted: bool) -> LegacyMediaAsset {
    LegacyMediaAsset {
        id,
        storage_key: format!("media/{id}"),
        original_name: "image.png".into(),
        media_type: "image/png".into(),
        byte_size: 1,
        checksum: "checksum".into(),
        metadata: json!({}),
        created_at: timestamp(),
        deleted_at: deleted.then(timestamp),
    }
}

fn with_media(mut entry: LegacyContentEntry, media_id: Uuid) -> LegacyContentEntry {
    entry
        .payload
        .pointer_mut("/body/doc/attrs/pageSlots")
        .and_then(Value::as_object_mut)
        .expect("page slots")
        .insert(
            "media".into(),
            json!({
                "media": {
                    "asset": {
                        "assetId": media_id
                    },
                    "altText": "Diagram",
                    "decorative": false
                },
                "caption": null,
                "layout": "inline"
            }),
        );
    entry
}

#[test]
fn deleted_media_blocks_published_but_only_warns_for_draft() {
    let media_id = Uuid::from_u128(1_100);
    let draft = with_media(
        legacy_entry(Uuid::from_u128(1_101), "home", "draft", None),
        media_id,
    );
    let draft_report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![draft],
            media_assets: vec![asset(media_id, true)],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert!(draft_report.can_migrate);
    assert!(draft_report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::MissingMediaVersion
            && issue.severity == MigrationPreflightSeverity::Warning
    }));

    let mut published = with_media(
        legacy_entry(Uuid::from_u128(1_102), "home", "published", None),
        media_id,
    );
    published.published_revision = Some(1);
    published.payload["publishedRevision"] = json!(1);
    let revision = LegacyContentRevision {
        content_id: published.id,
        revision: 1,
        payload: published.payload.clone(),
        created_by: "tester".into(),
        created_at: timestamp(),
    };
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![published],
            content_revisions: vec![revision],
            media_assets: vec![asset(media_id, true)],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert!(report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::MissingMediaVersion
            && issue.severity == MigrationPreflightSeverity::Blocking
    }));
    assert!(!report.can_migrate);
}

#[test]
fn published_pointer_requires_an_immutable_revision() {
    let mut entry = legacy_entry(Uuid::from_u128(1_200), "home", "published", None);
    entry.published_revision = Some(99);
    entry.payload["publishedRevision"] = json!(99);
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![entry],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert!(report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::RevisionConversionFailed
            && issue.json_path.as_deref() == Some("publishedRevision")
    }));
    assert!(!report.can_migrate);
}

#[test]
fn existing_current_revision_must_match_the_working_payload() {
    let entry = legacy_entry(Uuid::from_u128(1_300), "home", "draft", None);
    let mut immutable = entry.payload.clone();
    immutable["title"] = json!("Different immutable title");
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![entry.clone()],
            content_revisions: vec![LegacyContentRevision {
                content_id: entry.id,
                revision: entry.current_revision,
                payload: immutable,
                created_by: "tester".into(),
                created_at: timestamp(),
            }],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert!(report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::RevisionConversionFailed
            && issue.json_path.as_deref() == Some("currentRevision")
    }));
}

#[test]
fn current_news_metadata_must_match_its_immutable_row() {
    let entry = legacy_entry(Uuid::from_u128(1_400), "news", "draft", None);
    let working = LegacyNewsWorking {
        content_id: entry.id,
        content_kind: "news".into(),
        category: "Working".into(),
        author_display_name: Some("Editor".into()),
        cover_media_asset_id: None,
        featured: false,
        publication_at: None,
        reading_minutes: Some(2),
        data_origin: entry.data_origin.clone(),
        updated_at: timestamp(),
    };
    let immutable = LegacyNews {
        content_id: entry.id,
        revision: entry.current_revision,
        content_kind: working.content_kind.clone(),
        category: "Different".into(),
        author_display_name: working.author_display_name.clone(),
        cover_media_asset_id: working.cover_media_asset_id,
        featured: working.featured,
        publication_at: working.publication_at,
        reading_minutes: working.reading_minutes,
        data_origin: working.data_origin.clone(),
    };
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![entry.clone()],
            content_revisions: vec![LegacyContentRevision {
                content_id: entry.id,
                revision: entry.current_revision,
                payload: entry.payload.clone(),
                created_by: "tester".into(),
                created_at: timestamp(),
            }],
            news_working: vec![working],
            news: vec![immutable],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert!(report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::RevisionConversionFailed
            && issue.json_path.as_deref() == Some("newsWorking")
    }));
}

#[test]
fn product_asset_reference_requires_an_existing_product() {
    let media_id = Uuid::from_u128(1_500);
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            media_assets: vec![asset(media_id, false)],
            asset_references: vec![LegacyAssetReference {
                id: Uuid::from_u128(1_501),
                media_asset_id: media_id,
                content_id: None,
                content_revision: None,
                product_id: Some(Uuid::from_u128(1_599)),
                product_revision: Some(1),
                general_information_id: None,
                general_information_revision: None,
                usage: "hero".into(),
                locale: Some("en".into()),
                alt_text: Some("Product".into()),
                sort_order: 0,
            }],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    assert!(report.issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::InvalidRelation
            && issue.json_path.as_deref() == Some("owner")
    }));
}

#[test]
fn candidate_content_links_require_an_existing_content_target() {
    let mut entry = legacy_entry(Uuid::from_u128(1_600), "home", "draft", None);
    entry.payload["body"]["doc"]["attrs"]["pageSlots"]["primaryCta"] = json!({
        "title": "Related content",
        "label": "Read",
        "href": "/en/related"
    });
    let (record, conversion_issues) = convert_content(
        ContentSource {
            entity_id: entry.id,
            revision: entry.current_revision,
            role: CmsPreflightRecordRole::Working,
            kind: entry.kind.clone(),
            slug: entry.slug.clone(),
            locale: entry.locale.clone(),
            title: entry.title.clone(),
            is_published_revision: false,
            is_placeholder: false,
            allows_lossy_placeholder_cleanup: false,
            scheduled: false,
            payload: entry.payload.clone(),
            current_relations: Vec::new(),
            relation_history_unavailable: false,
        },
        Default::default(),
    );
    assert!(conversion_issues
        .iter()
        .all(|issue| issue.severity != MigrationPreflightSeverity::Blocking));
    let mut record = record.expect("valid candidate");
    let missing_id = Uuid::from_u128(1_699);
    let cta = record
        .candidate
        .composition
        .blocks
        .iter_mut()
        .find_map(|block| match block {
            ContentBlock::Cta(cta) => Some(cta),
            _ => None,
        })
        .expect("CTA block");
    cta.action.target = LinkTargetReference::Content {
        content_id: missing_id,
    };
    let mut records = vec![record];
    let snapshot = LegacySnapshot {
        content_entries: vec![entry],
        ..LegacySnapshot::default()
    };
    let mut issues = Vec::new();

    validate_candidate_targets(&mut records, &snapshot, &mut issues);

    assert!(records.is_empty());
    assert!(issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::MissingRelationTarget
            && issue
                .json_path
                .as_deref()
                .is_some_and(|path| path.ends_with("action.target"))
    }));
}
