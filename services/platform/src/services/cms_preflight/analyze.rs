use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::models::{
    ContentRelationReference, MigrationPreflightCounts, MigrationPreflightReport,
    RelationTargetReference, CMS_V2_SCHEMA_VERSION,
};

use super::{
    conversion::{convert_content, issue, ContentSource, IssueContext},
    counts::converted_counts,
    general_information_records::convert_general_information_records,
    integrity::validate_candidate_targets,
    news::{check_orphan_news, news_metadata_equal, news_revision_fields, news_working_fields},
    ordering::{sort_issues, sort_snapshot},
    references::convert_references,
    routes::validate_public_routes,
    singletons::validate_singleton_templates,
    support::stable_id,
    types::*,
};

#[derive(Clone, Debug)]
pub struct LegacyMigrationPlan {
    pub report: MigrationPreflightReport,
    pub records: Vec<CmsPreflightRecord>,
}

pub fn analyze_legacy_snapshot(
    snapshot: LegacySnapshot,
    generated_at: DateTime<Utc>,
) -> MigrationPreflightReport {
    plan_legacy_snapshot(snapshot, generated_at).report
}

pub fn plan_legacy_snapshot(
    mut snapshot: LegacySnapshot,
    generated_at: DateTime<Utc>,
) -> LegacyMigrationPlan {
    sort_snapshot(&mut snapshot);
    let content_by_id = snapshot
        .content_entries
        .iter()
        .map(|value| (value.id, value))
        .collect::<BTreeMap<_, _>>();
    let revision_keys = snapshot
        .content_revisions
        .iter()
        .map(|value| (value.content_id, value.revision))
        .collect::<BTreeSet<_>>();
    let revisions_by_key = snapshot
        .content_revisions
        .iter()
        .map(|value| ((value.content_id, value.revision), value))
        .collect::<BTreeMap<_, _>>();
    let news_working = snapshot
        .news_working
        .iter()
        .map(|value| (value.content_id, value))
        .collect::<BTreeMap<_, _>>();
    let news_revisions = snapshot
        .news
        .iter()
        .map(|value| ((value.content_id, value.revision), value))
        .collect::<BTreeMap<_, _>>();
    let media_ids = snapshot
        .media_assets
        .iter()
        .map(|value| value.id)
        .collect::<BTreeSet<_>>();
    let mut records = Vec::new();
    let mut issues = Vec::new();

    for working in &snapshot.news_working {
        let current_revision = content_by_id
            .get(&working.content_id)
            .map(|entry| entry.current_revision);
        if current_revision
            .and_then(|revision| news_revisions.get(&(working.content_id, revision)))
            .is_some_and(|revision| !news_metadata_equal(working, revision))
        {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::News,
                Some(working.content_id),
                current_revision,
                "newsWorking",
                "Working News metadata conflicts with the immutable row carrying the current revision.",
            ));
        }
    }

    for entry in &snapshot.content_entries {
        if revisions_by_key
            .get(&(entry.id, entry.current_revision))
            .is_some_and(|revision| revision.payload != entry.payload)
        {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::Content,
                Some(entry.id),
                Some(entry.current_revision),
                "currentRevision",
                "Working payload conflicts with the immutable row carrying the same revision number.",
            ));
        }
        if entry
            .published_revision
            .is_some_and(|revision| !revision_keys.contains(&(entry.id, revision)))
        {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::Content,
                Some(entry.id),
                entry.published_revision,
                "publishedRevision",
                "Published content pointer has no immutable content_revisions row.",
            ));
        }
    }

    for entry in &snapshot.content_entries {
        check_working_payload(entry, &mut issues);
        let mut seed = common_seed(&snapshot);
        if entry.kind == "news" {
            match news_working.get(&entry.id) {
                Some(metadata) => {
                    seed.extend(news_working_fields(
                        entry,
                        metadata,
                        &media_ids,
                        &mut issues,
                    ));
                }
                None => issues.push(issue(
                    CmsPreflightSeverity::Blocking,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    CmsPreflightSource::News,
                    Some(entry.id),
                    Some(entry.current_revision),
                    "newsWorking",
                    "News content has no current news_working metadata.",
                )),
            }
        }
        add_download_asset_seed(
            &snapshot,
            entry.id,
            entry.current_revision,
            &mut seed,
            &mut issues,
        );
        let source = ContentSource {
            entity_id: entry.id,
            revision: entry.current_revision,
            role: CmsPreflightRecordRole::Working,
            kind: entry.kind.clone(),
            slug: entry.slug.clone(),
            locale: entry.locale.clone(),
            title: entry.title.clone(),
            is_published_revision: entry.published_revision == Some(entry.current_revision),
            is_placeholder: entry.is_placeholder,
            allows_lossy_placeholder_cleanup: entry.is_placeholder
                && entry.data_origin == "developmentFixture",
            scheduled: entry.scheduled_for.is_some() || entry.status == "scheduled",
            payload: entry.payload.clone(),
            current_relations: current_relations(entry, &snapshot.content_relations),
            relation_history_unavailable: false,
        };
        let (record, mut found) = convert_content(source, seed);
        records.extend(record);
        issues.append(&mut found);
    }

    for revision in &snapshot.content_revisions {
        let Some(parent) = content_by_id.get(&revision.content_id) else {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::ContentRevision,
                Some(revision.content_id),
                Some(revision.revision),
                "contentId",
                "Content revision has no parent content entry.",
            ));
            continue;
        };
        let Some(source) = revision_source(parent, revision, &snapshot, &mut issues) else {
            continue;
        };
        let mut seed = common_seed(&snapshot);
        if source.kind == "news" {
            match news_revisions.get(&(revision.content_id, revision.revision)) {
                Some(metadata) => {
                    seed.extend(news_revision_fields(
                        parent,
                        metadata,
                        &media_ids,
                        &mut issues,
                    ));
                }
                None => issues.push(issue(
                    CmsPreflightSeverity::Blocking,
                    CmsPreflightIssueCode::RevisionConversionFailed,
                    CmsPreflightSource::News,
                    Some(revision.content_id),
                    Some(revision.revision),
                    "news",
                    "News revision has no matching immutable metadata.",
                )),
            }
        }
        add_download_asset_seed(
            &snapshot,
            revision.content_id,
            revision.revision,
            &mut seed,
            &mut issues,
        );
        let (record, mut found) = convert_content(source, seed);
        records.extend(record);
        issues.append(&mut found);
    }
    check_orphan_news(&snapshot, &content_by_id, &revision_keys, &mut issues);
    convert_general_information_records(&snapshot, &mut records, &mut issues);
    validate_singleton_templates(&snapshot, &records, &mut issues);
    validate_candidate_targets(&mut records, &snapshot, &mut issues);
    let (media_versions, relations, asset_references) = convert_references(&snapshot, &mut issues);
    let convertible_routes = validate_public_routes(&snapshot, &records, &mut issues);
    sort_issues(&mut issues);

    let scanned = MigrationPreflightCounts {
        content_entries: snapshot.content_entries.len() as u64,
        content_revisions: snapshot.content_revisions.len() as u64,
        news_entries: (snapshot.news.len() + snapshot.news_working.len()) as u64,
        general_information_entries: (snapshot.general_information.len()
            + snapshot.general_information_revisions.len())
            as u64,
        media_assets: snapshot.media_assets.len() as u64,
        media_references: snapshot.asset_references.len() as u64,
        relations: snapshot.content_relations.len() as u64,
        public_routes: snapshot.public_routes.len() as u64,
    };
    let converted = converted_counts(
        &snapshot,
        &records,
        &content_by_id,
        media_versions.len(),
        asset_references.len(),
        relations.len(),
        convertible_routes,
    );
    let blocking_issue_count = issues
        .iter()
        .filter(|value| value.severity == CmsPreflightSeverity::Blocking)
        .count() as u64;
    let warning_count = issues.len() as u64 - blocking_issue_count;
    let report = MigrationPreflightReport {
        target_schema_version: CMS_V2_SCHEMA_VERSION,
        generated_at,
        can_migrate: blocking_issue_count == 0 && scanned == converted,
        scanned,
        convertible: converted,
        blocking_issue_count,
        warning_count,
        issues,
    };
    LegacyMigrationPlan { report, records }
}

fn common_seed(snapshot: &LegacySnapshot) -> Map<String, Value> {
    Map::from_iter([(
        "productIds".into(),
        Value::Array(
            snapshot
                .products
                .iter()
                .map(|value| json!(value.id))
                .collect(),
        ),
    )])
}

fn current_relations(
    entry: &LegacyContentEntry,
    relations: &[LegacyContentRelation],
) -> Vec<ContentRelationReference> {
    let context = IssueContext {
        source: CmsPreflightSource::Content,
        entity_id: entry.id,
        revision: entry.current_revision,
    };
    relations
        .iter()
        .filter(|value| value.from_type == "content" && value.from_id == entry.id)
        .filter_map(|value| {
            let target = match value.to_type.as_str() {
                "content" => RelationTargetReference::Content {
                    content_id: value.to_id,
                },
                "product" => RelationTargetReference::Product {
                    product_id: value.to_id,
                },
                _ => return None,
            };
            Some(ContentRelationReference {
                id: stable_id(
                    context,
                    &format!(
                        "databaseRelation.{}.{}.{}",
                        value.relation_type, value.to_type, value.to_id
                    ),
                ),
                slot: value.relation_type.clone(),
                target,
            })
        })
        .collect()
}

fn check_working_payload(entry: &LegacyContentEntry, issues: &mut Vec<CmsPreflightIssue>) {
    let fields = [
        ("id", json!(entry.id)),
        ("kind", json!(entry.kind)),
        ("slug", json!(entry.slug)),
        ("locale", json!(entry.locale)),
        ("title", json!(entry.title)),
        ("status", json!(entry.status)),
        ("isPlaceholder", json!(entry.is_placeholder)),
        ("currentRevision", json!(entry.current_revision)),
        ("publishedRevision", json!(entry.published_revision)),
        ("scheduledFor", json!(entry.scheduled_for)),
    ];
    for (field, expected) in fields {
        if entry.payload.get(field) != Some(&expected) {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::InvalidLegacyPayload,
                CmsPreflightSource::Content,
                Some(entry.id),
                Some(entry.current_revision),
                &format!("payload.{field}"),
                "Working payload disagrees with its authoritative content_entries column.",
            ));
        }
    }
}

fn revision_source(
    parent: &LegacyContentEntry,
    revision: &LegacyContentRevision,
    snapshot: &LegacySnapshot,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<ContentSource> {
    let Some(object) = revision.payload.as_object() else {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::InvalidLegacyPayload,
            CmsPreflightSource::ContentRevision,
            Some(revision.content_id),
            Some(revision.revision),
            "payload",
            "Content revision payload is not an object.",
        ));
        return None;
    };
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let kind = object.get("kind").and_then(Value::as_str);
    let slug = object.get("slug").and_then(Value::as_str);
    let locale = object.get("locale").and_then(Value::as_str);
    let title = object.get("title").and_then(Value::as_str);
    let status = object.get("status").and_then(Value::as_str);
    let placeholder = object.get("isPlaceholder").and_then(Value::as_bool);
    let current_revision = object.get("currentRevision").and_then(Value::as_i64);
    if id.is_none()
        || kind.is_none()
        || slug.is_none()
        || locale.is_none()
        || title.is_none()
        || status.is_none()
        || placeholder.is_none()
        || current_revision.is_none()
    {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::InvalidLegacyPayload,
            CmsPreflightSource::ContentRevision,
            Some(revision.content_id),
            Some(revision.revision),
            "payload",
            "Content revision is missing required serialized identity fields.",
        ));
        return None;
    }
    if id != Some(revision.content_id)
        || kind != Some(parent.kind.as_str())
        || current_revision != Some(revision.revision)
    {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::RevisionConversionFailed,
            CmsPreflightSource::ContentRevision,
            Some(revision.content_id),
            Some(revision.revision),
            "payload",
            "Content revision identity differs from its row key or parent kind.",
        ));
    }
    let has_relations = snapshot
        .content_relations
        .iter()
        .any(|value| value.from_type == "content" && value.from_id == revision.content_id);
    Some(ContentSource {
        entity_id: revision.content_id,
        revision: revision.revision,
        role: CmsPreflightRecordRole::Revision,
        kind: kind.expect("checked").into(),
        slug: slug.expect("checked").into(),
        locale: locale.expect("checked").into(),
        title: title.expect("checked").into(),
        is_published_revision: parent.published_revision == Some(revision.revision),
        is_placeholder: placeholder.expect("checked"),
        allows_lossy_placeholder_cleanup: placeholder == Some(true)
            && parent.data_origin == "developmentFixture",
        scheduled: status == Some("scheduled")
            || object
                .get("scheduledFor")
                .is_some_and(|value| !value.is_null()),
        payload: revision.payload.clone(),
        current_relations: Vec::new(),
        relation_history_unavailable: has_relations,
    })
}

fn add_download_asset_seed(
    snapshot: &LegacySnapshot,
    content_id: Uuid,
    revision: i64,
    seed: &mut Map<String, Value>,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let references = snapshot
        .asset_references
        .iter()
        .filter(|value| {
            value.content_id == Some(content_id)
                && value.content_revision == Some(revision)
                && matches!(
                    value.usage.as_str(),
                    "download" | "datasheet" | "cad" | "certificate"
                )
        })
        .collect::<Vec<_>>();
    if references.len() > 1 {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::InvalidRelation,
            CmsPreflightSource::Media,
            Some(content_id),
            Some(revision),
            "assetReferences",
            "Download has multiple primary file references; selection is ambiguous.",
        ));
    } else if let Some(reference) = references.first() {
        seed.insert(
            "asset".into(),
            json!({
                "assetId": reference.media_asset_id,
            }),
        );
    }
}
