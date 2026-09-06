use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::models::{MigrationPreflightIssueCode, MigrationPreflightSource};

use super::{
    analyze_legacy_snapshot,
    routes::validate_public_routes,
    types::{
        CmsPreflightIssue, LegacyContentEntry, LegacyContentRevision, LegacyProductIdentity,
        LegacyPublicRoute, LegacySnapshot,
    },
};

fn timestamp() -> DateTime<Utc> {
    "2026-09-04T00:00:00Z".parse().expect("timestamp")
}

#[test]
fn route_report_covers_duplicate_owner_canonical_and_hard_noindex() {
    let content_id = Uuid::from_u128(800);
    let expected = "/en/expected";
    let payload = json!({
        "id": content_id,
        "kind": "home",
        "slug": "expected",
        "locale": "en",
        "title": "Expected",
        "summary": null,
        "body": {
            "schemaVersion": 1,
            "doc": {
                "type": "doc",
                "attrs": {"pageSlots": {
                    "templateKey": "productIndex",
                    "hero": {"title": "Expected"}
                }},
                "content": [{"type": "paragraph"}]
            }
        },
        "seo": {"title": null, "description": null, "canonicalPath": expected, "indexable": true},
        "status": "published",
        "isPlaceholder": true,
        "currentRevision": 1,
        "publishedRevision": 1,
        "scheduledFor": null,
        "updatedAt": timestamp()
    });
    let entry = LegacyContentEntry {
        id: content_id,
        kind: "home".into(),
        slug: "expected".into(),
        locale: "en".into(),
        title: "Expected".into(),
        status: "published".into(),
        is_placeholder: true,
        data_origin: "developmentFixture".into(),
        current_revision: 1,
        published_revision: Some(1),
        scheduled_for: None,
        payload: payload.clone(),
        updated_at: timestamp(),
    };
    let revision = LegacyContentRevision {
        content_id,
        revision: 1,
        payload,
        created_by: "tester".into(),
        created_at: timestamp(),
    };
    let duplicate_path = "/en/wrong";
    let content_route = LegacyPublicRoute {
        id: Uuid::from_u128(801),
        entity_type: "content".into(),
        entity_id: content_id,
        locale: "en".into(),
        canonical_path: duplicate_path.into(),
        indexable: true,
    };
    let orphan_route = LegacyPublicRoute {
        id: Uuid::from_u128(802),
        entity_type: "product".into(),
        entity_id: Uuid::from_u128(899),
        locale: "en".into(),
        canonical_path: duplicate_path.into(),
        indexable: false,
    };
    let report = analyze_legacy_snapshot(
        LegacySnapshot {
            content_entries: vec![entry],
            content_revisions: vec![revision],
            public_routes: vec![content_route, orphan_route],
            ..LegacySnapshot::default()
        },
        timestamp(),
    );
    for code in [
        MigrationPreflightIssueCode::DuplicatePublicPath,
        MigrationPreflightIssueCode::RouteOwnershipMismatch,
        MigrationPreflightIssueCode::CanonicalPathMismatch,
    ] {
        assert!(report.issues.iter().any(|issue| issue.code == code));
    }
    assert!(report.issues.iter().any(|issue| {
        issue.source == MigrationPreflightSource::Route
            && issue.json_path.as_deref() == Some("indexable")
    }));
    assert!(!report.can_migrate);
}

#[test]
fn product_route_requires_exact_verified_published_localization() {
    let product_id = Uuid::from_u128(900);
    let route = LegacyPublicRoute {
        id: Uuid::from_u128(901),
        entity_type: "product".into(),
        entity_id: product_id,
        locale: "en".into(),
        canonical_path: "/en/products/axial/model".into(),
        indexable: true,
    };
    let product = LegacyProductIdentity {
        id: product_id,
        locale: "en".into(),
        published_revision: Some(3),
        has_verified_published_localization: true,
        canonical_path: Some(route.canonical_path.clone()),
        is_placeholder: false,
        indexable: true,
    };
    let mut snapshot = LegacySnapshot {
        products: vec![product],
        public_routes: vec![route],
        ..LegacySnapshot::default()
    };
    let mut issues: Vec<CmsPreflightIssue> = Vec::new();
    assert_eq!(validate_public_routes(&snapshot, &[], &mut issues), 1);
    assert!(issues.is_empty());

    snapshot.products[0].has_verified_published_localization = false;
    let mut issues = Vec::new();
    assert_eq!(validate_public_routes(&snapshot, &[], &mut issues), 0);
    assert!(issues.iter().any(|issue| {
        issue.code == MigrationPreflightIssueCode::RouteOwnershipMismatch
            && issue.source == MigrationPreflightSource::Route
    }));
}

#[test]
fn loader_is_statically_read_only_and_covers_every_preflight_table() {
    let source = include_str!("load.rs");
    assert!(source.contains("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY"));
    assert!(source.contains("transaction.rollback().await"));
    for table in [
        "content_entries",
        "content_revisions",
        "news",
        "news_working",
        "general_information",
        "general_information_revisions",
        "content_relations",
        "media_assets",
        "asset_references",
        "public_routes",
        "products",
        "product_localizations",
    ] {
        assert!(source.contains(table), "missing read for {table}");
    }
    for mutation in ["INSERT ", "UPDATE ", "DELETE ", "TRUNCATE ", "ALTER TABLE"] {
        assert!(!source.contains(mutation), "loader contains {mutation}");
    }
}
