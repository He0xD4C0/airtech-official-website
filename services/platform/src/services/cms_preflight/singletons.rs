use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    models::{CmsContentKind, ContentTemplateKey},
    services::cms_templates::template_definition,
};

use super::{conversion::issue, types::*};

pub(super) fn validate_singleton_templates(
    snapshot: &LegacySnapshot,
    records: &[CmsPreflightRecord],
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let active = records
        .iter()
        .filter(|record| record.role == CmsPreflightRecordRole::Working)
        .filter(|record| !is_archived(snapshot, record))
        .filter(|record| {
            template_definition(record.candidate.template_key)
                .is_some_and(|definition| definition.singleton_per_locale)
        })
        .collect::<Vec<_>>();
    let mut owners = HashMap::<(ContentTemplateKey, String), BTreeSet<uuid::Uuid>>::new();
    for record in &active {
        owners
            .entry((
                record.candidate.template_key,
                record.candidate.locale.clone(),
            ))
            .or_default()
            .insert(record.entity_id);
    }
    let conflicts = owners
        .into_iter()
        .filter_map(|(key, owners)| (owners.len() > 1).then_some(key))
        .collect::<HashSet<_>>();

    for record in active.into_iter().filter(|record| {
        conflicts.contains(&(
            record.candidate.template_key,
            record.candidate.locale.clone(),
        ))
    }) {
        issues.push(issue(
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::RevisionConversionFailed,
            source(record),
            Some(record.entity_id),
            Some(record.source_revision),
            "templateKey",
            "Multiple active content entries claim a singleton template in the same locale.",
        ));
    }
}

fn is_archived(snapshot: &LegacySnapshot, record: &CmsPreflightRecord) -> bool {
    if record.candidate.kind == CmsContentKind::GeneralInformation {
        snapshot
            .general_information
            .iter()
            .find(|entry| entry.id == record.entity_id)
            .is_some_and(|entry| entry.status == "archived")
    } else {
        snapshot
            .content_entries
            .iter()
            .find(|entry| entry.id == record.entity_id)
            .is_some_and(|entry| entry.status == "archived")
    }
}

fn source(record: &CmsPreflightRecord) -> CmsPreflightSource {
    if record.candidate.kind == CmsContentKind::GeneralInformation {
        CmsPreflightSource::GeneralInformation
    } else {
        CmsPreflightSource::Content
    }
}
