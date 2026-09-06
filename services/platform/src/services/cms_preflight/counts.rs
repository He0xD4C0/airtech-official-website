use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use crate::models::{CmsContentKind, MigrationPreflightCounts};

use super::types::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn converted_counts(
    snapshot: &LegacySnapshot,
    records: &[CmsPreflightRecord],
    content_by_id: &BTreeMap<Uuid, &LegacyContentEntry>,
    media: usize,
    media_references: usize,
    relations: usize,
    public_routes: usize,
) -> MigrationPreflightCounts {
    let content_entries = records
        .iter()
        .filter(|record| {
            record.role == CmsPreflightRecordRole::Working
                && record.candidate.kind != CmsContentKind::GeneralInformation
        })
        .count();
    let content_revisions = records
        .iter()
        .filter(|record| {
            record.role == CmsPreflightRecordRole::Revision
                && record.candidate.kind != CmsContentKind::GeneralInformation
        })
        .count();
    let news_keys = records
        .iter()
        .filter(|record| record.candidate.kind == CmsContentKind::News)
        .map(|record| (record.entity_id, record.source_revision, record.role))
        .collect::<BTreeSet<_>>();
    let news_entries = snapshot
        .news_working
        .iter()
        .filter(|value| {
            news_keys.contains(&(
                value.content_id,
                content_by_id
                    .get(&value.content_id)
                    .map(|entry| entry.current_revision)
                    .unwrap_or_default(),
                CmsPreflightRecordRole::Working,
            ))
        })
        .count()
        + snapshot
            .news
            .iter()
            .filter(|value| {
                news_keys.contains(&(
                    value.content_id,
                    value.revision,
                    CmsPreflightRecordRole::Revision,
                ))
            })
            .count();
    MigrationPreflightCounts {
        content_entries: content_entries as u64,
        content_revisions: content_revisions as u64,
        news_entries: news_entries as u64,
        general_information_entries: records
            .iter()
            .filter(|record| record.candidate.kind == CmsContentKind::GeneralInformation)
            .count() as u64,
        media_assets: media as u64,
        media_references: media_references as u64,
        relations: relations as u64,
        public_routes: public_routes as u64,
    }
}
