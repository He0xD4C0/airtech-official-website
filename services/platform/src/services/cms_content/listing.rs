use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::models::{CmsContentKind, CmsPublicationStatusV2, ContentRecordV2};

use super::enum_label;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentSortField {
    UpdatedAt,
    Title,
    Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Server-side list filter for the unified CMS content collection.
#[derive(Clone, Debug)]
pub struct ContentListFilter {
    pub query: Option<String>,
    pub kinds: Vec<CmsContentKind>,
    pub status: Option<CmsPublicationStatusV2>,
    pub sort: ContentSortField,
    pub direction: SortDirection,
}

impl Default for ContentListFilter {
    fn default() -> Self {
        Self {
            query: None,
            kinds: Vec::new(),
            status: None,
            sort: ContentSortField::UpdatedAt,
            direction: SortDirection::Desc,
        }
    }
}

impl ContentListFilter {
    /// Stable signature of every filter and sort dimension. Page cursors are
    /// bound to it, so a cursor cannot be replayed after the filters change.
    pub fn cursor_scope(&self) -> String {
        let mut kinds: Vec<String> = self.kinds.iter().copied().map(enum_label).collect();
        kinds.sort();
        format!(
            "q={}|kinds={}|status={}|sort={}|direction={}",
            self.query.as_deref().unwrap_or(""),
            kinds.join(","),
            self.status.map(enum_label).unwrap_or_default(),
            sort_label(self.sort),
            direction_label(self.direction),
        )
    }
}

/// Filtered, sorted records plus the counts the list UI renders.
#[derive(Debug)]
pub struct ContentListOutcome {
    pub records: Vec<ContentRecordV2>,
    /// Records per content kind across the whole `query + status` selection.
    /// The `kind` filter is intentionally ignored so type tabs stay complete.
    pub counts: BTreeMap<String, usize>,
    /// Total of the `query + status` selection, independent of paging.
    pub total: usize,
}

pub fn apply_filter(
    records: Vec<ContentRecordV2>,
    filter: &ContentListFilter,
) -> ContentListOutcome {
    let mut matching: Vec<ContentRecordV2> = records
        .into_iter()
        .filter(|record| matches_query(record, filter.query.as_deref()))
        .filter(|record| filter.status.is_none_or(|status| record.status == status))
        .collect();

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for record in &matching {
        *counts.entry(enum_label(record.draft.kind)).or_insert(0) += 1;
    }
    let total = matching.len();

    if !filter.kinds.is_empty() {
        matching.retain(|record| filter.kinds.contains(&record.draft.kind));
    }
    sort_records(&mut matching, filter);

    ContentListOutcome {
        records: matching,
        counts,
        total,
    }
}

fn matches_query(record: &ContentRecordV2, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return true;
    }
    if record.draft.title.to_lowercase().contains(&needle) {
        return true;
    }
    record
        .draft
        .slug
        .as_deref()
        .is_some_and(|slug| slug.to_lowercase().contains(&needle))
}

fn sort_records(records: &mut [ContentRecordV2], filter: &ContentListFilter) {
    records.sort_by(|left, right| {
        let ordering = match filter.sort {
            ContentSortField::UpdatedAt => left.updated_at.cmp(&right.updated_at),
            ContentSortField::Title => compare_text(&left.draft.title, &right.draft.title),
            ContentSortField::Kind => {
                compare_text(&enum_label(left.draft.kind), &enum_label(right.draft.kind))
            }
        };
        let ordering = match filter.direction {
            SortDirection::Asc => ordering,
            SortDirection::Desc => ordering.reverse(),
        };
        ordering.then_with(|| left.id.cmp(&right.id))
    });
}

fn compare_text(left: &str, right: &str) -> Ordering {
    match left.to_lowercase().cmp(&right.to_lowercase()) {
        Ordering::Equal => left.cmp(right),
        ordering => ordering,
    }
}

fn sort_label(sort: ContentSortField) -> &'static str {
    match sort {
        ContentSortField::UpdatedAt => "updatedAt",
        ContentSortField::Title => "title",
        ContentSortField::Kind => "kind",
    }
}

fn direction_label(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Asc => "asc",
        SortDirection::Desc => "desc",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use uuid::Uuid;

    use super::*;

    fn type_fields(kind: &str) -> Value {
        match kind {
            "page" => json!({"type": "page"}),
            "home" => json!({"type": "home"}),
            "navigation" => json!({"type": "navigation", "items": []}),
            "footer" => json!({"type": "footer", "columns": [], "legalLinks": []}),
            other => panic!("unsupported test content kind {other}"),
        }
    }

    fn template_for(kind: &str) -> &'static str {
        match kind {
            "page" => "productIndex",
            "home" => "home",
            "navigation" => "navigation",
            "footer" => "footer",
            other => panic!("unsupported test content kind {other}"),
        }
    }

    fn record(
        id: u128,
        kind: &str,
        status: &str,
        title: &str,
        slug: &str,
        updated_at: &str,
    ) -> ContentRecordV2 {
        serde_json::from_value(json!({
            "id": Uuid::from_u128(id),
            "status": status,
            "draft": {
                "schemaVersion": 2,
                "kind": kind,
                "locale": "en",
                "templateKey": template_for(kind),
                "title": title,
                "slug": slug,
                "summary": null,
                "isPlaceholder": false,
                "typeFields": type_fields(kind),
                "body": null,
                "composition": {"blocks": []},
                "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
                "relations": [],
                "draftVersion": 1
            },
            "latestRevision": null,
            "publishedRevision": null,
            "createdAt": updated_at,
            "updatedAt": updated_at,
            "updatedBy": "listing-tests"
        }))
        .expect("content record fixture")
    }

    fn ids(outcome: &ContentListOutcome) -> Vec<u128> {
        outcome
            .records
            .iter()
            .map(|record| record.id.as_u128())
            .collect()
    }

    #[test]
    fn filters_by_query_status_and_kind_while_counts_ignore_the_kind_filter() {
        let records = vec![
            record(
                1,
                "page",
                "published",
                "Site overview",
                "site-overview",
                "2026-09-01T00:00:00Z",
            ),
            record(
                2,
                "page",
                "draft",
                "Fan selection",
                "fan-selection",
                "2026-09-02T00:00:00Z",
            ),
            record(
                3,
                "navigation",
                "published",
                "Site navigation",
                "site-navigation",
                "2026-09-03T00:00:00Z",
            ),
            record(
                4,
                "footer",
                "published",
                "Site footer",
                "site-footer",
                "2026-09-04T00:00:00Z",
            ),
            record(
                5,
                "home",
                "published",
                "AIRTEKPOWER home",
                "home",
                "2026-09-05T00:00:00Z",
            ),
        ];

        let filter = ContentListFilter {
            query: Some("site".into()),
            kinds: vec![CmsContentKind::Navigation],
            status: Some(CmsPublicationStatusV2::Published),
            ..ContentListFilter::default()
        };
        let outcome = apply_filter(records, &filter);

        assert_eq!(ids(&outcome), vec![3]);
        assert_eq!(outcome.total, 3);
        assert_eq!(outcome.counts.get("page"), Some(&1));
        assert_eq!(outcome.counts.get("navigation"), Some(&1));
        assert_eq!(outcome.counts.get("footer"), Some(&1));
        assert_eq!(outcome.counts.get("home"), None);
    }

    #[test]
    fn matches_slug_case_insensitively_and_sorts_titles_stably() {
        let records = vec![
            record(
                1,
                "page",
                "draft",
                "beta page",
                "BETA-page",
                "2026-09-01T00:00:00Z",
            ),
            record(
                2,
                "page",
                "draft",
                "Alpha page",
                "alpha-page",
                "2026-09-02T00:00:00Z",
            ),
            record(
                3,
                "footer",
                "draft",
                "Alpha page",
                "alpha-page-duplicate",
                "2026-09-03T00:00:00Z",
            ),
        ];

        let by_query = ContentListFilter {
            query: Some("beta-p".into()),
            ..ContentListFilter::default()
        };
        assert_eq!(ids(&apply_filter(records.clone(), &by_query)), vec![1]);

        let by_title = ContentListFilter {
            sort: ContentSortField::Title,
            direction: SortDirection::Asc,
            ..ContentListFilter::default()
        };
        let outcome = apply_filter(records, &by_title);
        assert_eq!(ids(&outcome), vec![2, 3, 1]);
    }

    #[test]
    fn counts_cover_the_whole_collection_beyond_one_page() {
        let mut records = Vec::new();
        for index in 0..120u128 {
            records.push(record(
                index + 1,
                "page",
                "draft",
                &format!("Page {index:03}"),
                &format!("page-{index:03}"),
                "2026-09-01T00:00:00Z",
            ));
        }
        records.push(record(
            500,
            "home",
            "draft",
            "Home",
            "home",
            "2026-09-01T00:00:00Z",
        ));
        records.push(record(
            501,
            "footer",
            "draft",
            "Footer",
            "footer",
            "2026-09-01T00:00:00Z",
        ));

        let outcome = apply_filter(records, &ContentListFilter::default());

        assert_eq!(outcome.records.len(), 122);
        assert_eq!(outcome.total, 122);
        assert_eq!(outcome.counts.get("page"), Some(&120));
        assert_eq!(outcome.counts.get("home"), Some(&1));
        assert_eq!(outcome.counts.get("footer"), Some(&1));
        assert_eq!(outcome.counts.values().sum::<usize>(), outcome.total);
    }

    #[test]
    fn cursor_scope_changes_with_every_filter_dimension() {
        let base = ContentListFilter::default();
        let scoped = ContentListFilter {
            query: Some("pump".into()),
            kinds: vec![CmsContentKind::News],
            status: Some(CmsPublicationStatusV2::Published),
            sort: ContentSortField::Title,
            direction: SortDirection::Asc,
        };
        assert_ne!(base.cursor_scope(), scoped.cursor_scope());

        // Kind order must not change the signature.
        let mut reversed = scoped.clone();
        reversed.kinds = vec![CmsContentKind::News, CmsContentKind::Page];
        let mut reordered = scoped.clone();
        reordered.kinds = vec![CmsContentKind::Page, CmsContentKind::News];
        assert_eq!(reversed.cursor_scope(), reordered.cursor_scope());
    }
}
