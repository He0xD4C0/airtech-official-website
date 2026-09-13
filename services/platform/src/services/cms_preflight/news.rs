use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};
use uuid::Uuid;

use super::{conversion::issue, types::*};

pub(super) fn news_metadata_equal(working: &LegacyNewsWorking, revision: &LegacyNews) -> bool {
    working.content_kind == revision.content_kind
        && working.category == revision.category
        && working.author_display_name == revision.author_display_name
        && working.cover_media_asset_id == revision.cover_media_asset_id
        && working.featured == revision.featured
        && working.publication_at == revision.publication_at
        && working.reading_minutes == revision.reading_minutes
        && working.data_origin == revision.data_origin
}

pub(super) fn news_working_fields(
    entry: &LegacyContentEntry,
    metadata: &LegacyNewsWorking,
    media_ids: &BTreeSet<Uuid>,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Map<String, Value> {
    validate_news_metadata(
        entry,
        &metadata.content_kind,
        &metadata.category,
        metadata.reading_minutes,
        metadata.cover_media_asset_id,
        &metadata.data_origin,
        entry.current_revision,
        media_ids,
        issues,
    );
    news_fields(
        &metadata.category,
        metadata.author_display_name.as_deref(),
        metadata.cover_media_asset_id,
        metadata.featured,
        metadata.publication_at,
    )
}

pub(super) fn news_revision_fields(
    entry: &LegacyContentEntry,
    metadata: &LegacyNews,
    media_ids: &BTreeSet<Uuid>,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Map<String, Value> {
    validate_news_metadata(
        entry,
        &metadata.content_kind,
        &metadata.category,
        metadata.reading_minutes,
        metadata.cover_media_asset_id,
        &metadata.data_origin,
        metadata.revision,
        media_ids,
        issues,
    );
    news_fields(
        &metadata.category,
        metadata.author_display_name.as_deref(),
        metadata.cover_media_asset_id,
        metadata.featured,
        metadata.publication_at,
    )
}

pub(super) fn check_orphan_news(
    snapshot: &LegacySnapshot,
    content_by_id: &BTreeMap<Uuid, &LegacyContentEntry>,
    revision_keys: &BTreeSet<(Uuid, i64)>,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for metadata in &snapshot.news_working {
        if !content_by_id
            .get(&metadata.content_id)
            .is_some_and(|entry| entry.kind == "news")
        {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::News,
                Some(metadata.content_id),
                None,
                "contentId",
                "news_working metadata has no News content entry.",
            ));
        }
    }
    for metadata in &snapshot.news {
        if !revision_keys.contains(&(metadata.content_id, metadata.revision))
            || !content_by_id
                .get(&metadata.content_id)
                .is_some_and(|entry| entry.kind == "news")
        {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::News,
                Some(metadata.content_id),
                Some(metadata.revision),
                "contentId",
                "Immutable News metadata has no matching News content revision.",
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_news_metadata(
    entry: &LegacyContentEntry,
    content_kind: &str,
    category: &str,
    reading_minutes: Option<i32>,
    cover: Option<Uuid>,
    data_origin: &str,
    revision: i64,
    media_ids: &BTreeSet<Uuid>,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    if content_kind != "news" || category.trim().is_empty() || data_origin != entry.data_origin {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            CmsPreflightSource::News,
            Some(entry.id),
            Some(revision),
            "news",
            "News metadata is invalid or disagrees with its content entry.",
        ));
    }
    if reading_minutes.is_some_and(|minutes| minutes <= 0) {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            CmsPreflightSource::News,
            Some(entry.id),
            Some(revision),
            "news.readingMinutes",
            "News readingMinutes must be positive when present.",
        ));
    } else if reading_minutes.is_some() {
        issues.push(issue(
            CmsPreflightSeverity::Warning,
            CmsPreflightIssueCode::TypeFieldMismatch,
            CmsPreflightSource::News,
            Some(entry.id),
            Some(revision),
            "news.readingMinutes",
            "Legacy readingMinutes is removed because CMS V2 derives reading time from rich text.",
        ));
    }
    if cover.is_some_and(|id| !media_ids.contains(&id)) {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::MissingMediaAsset,
            CmsPreflightSource::News,
            Some(entry.id),
            Some(revision),
            "news.coverMediaAssetId",
            "News cover references a missing media asset.",
        ));
    }
}

fn news_fields(
    category: &str,
    author: Option<&str>,
    cover: Option<Uuid>,
    featured: bool,
    publication_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Map<String, Value> {
    let cover = cover.map(|asset_id| {
        json!({
            "asset": {
                "assetId": asset_id,
            },
            "altText": null,
            "decorative": false,
        })
    });
    json!({
        "category": category,
        "authorDisplayName": author,
        "cover": cover,
        "featured": featured,
        "publicationAt": publication_at,
    })
    .as_object()
    .cloned()
    .unwrap_or_default()
}
