use std::collections::BTreeMap;

use super::{
    conversion::issue,
    general_information::{convert_general_information, GeneralInformationSource},
    types::*,
};

pub(super) fn convert_general_information_records(
    snapshot: &LegacySnapshot,
    records: &mut Vec<CmsPreflightRecord>,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    let revisions = snapshot
        .general_information_revisions
        .iter()
        .map(|value| ((value.general_information_id, value.revision), value))
        .collect::<BTreeMap<_, _>>();
    let parents = snapshot
        .general_information
        .iter()
        .map(|value| (value.id, value))
        .collect::<BTreeMap<_, _>>();
    for entry in &snapshot.general_information {
        for pointer in [Some(entry.current_revision), entry.published_revision]
            .into_iter()
            .flatten()
        {
            if !revisions.contains_key(&(entry.id, pointer)) {
                issues.push(issue(
                    CmsPreflightSeverity::Blocking,
                    CmsPreflightIssueCode::RevisionConversionFailed,
                    CmsPreflightSource::GeneralInformation,
                    Some(entry.id),
                    Some(pointer),
                    "revision",
                    "General Information pointer has no immutable revision.",
                ));
            }
        }
        if revisions
            .get(&(entry.id, entry.current_revision))
            .is_some_and(|revision| revision.payload != entry.payload)
        {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::GeneralInformation,
                Some(entry.id),
                Some(entry.current_revision),
                "payload",
                "Working General Information differs from its current immutable revision.",
            ));
        }
        let source = GeneralInformationSource {
            id: entry.id,
            revision: entry.current_revision,
            role: CmsPreflightRecordRole::Working,
            scope: entry.scope.clone(),
            locale: entry.locale.clone(),
            is_published_revision: entry.published_revision == Some(entry.current_revision),
            is_placeholder: entry.is_placeholder,
            scheduled: entry.scheduled_for.is_some() || entry.status == "scheduled",
            payload: entry.payload.clone(),
        };
        let (record, mut found) = convert_general_information(source);
        records.extend(record);
        issues.append(&mut found);
    }
    for revision in &snapshot.general_information_revisions {
        let Some(parent) = parents.get(&revision.general_information_id) else {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::GeneralInformation,
                Some(revision.general_information_id),
                Some(revision.revision),
                "generalInformationId",
                "General Information revision has no parent.",
            ));
            continue;
        };
        if (parent.current_revision == revision.revision
            || parent.published_revision == Some(revision.revision))
            && (parent.locale != revision.locale
                || parent.is_placeholder != revision.is_placeholder
                || parent.data_origin != revision.data_origin)
        {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::RevisionConversionFailed,
                CmsPreflightSource::GeneralInformation,
                Some(revision.general_information_id),
                Some(revision.revision),
                "metadata",
                "Pointed General Information revision metadata differs from its parent.",
            ));
        }
        let source = GeneralInformationSource {
            id: revision.general_information_id,
            revision: revision.revision,
            role: CmsPreflightRecordRole::Revision,
            scope: parent.scope.clone(),
            locale: revision.locale.clone(),
            is_published_revision: parent.published_revision == Some(revision.revision),
            is_placeholder: revision.is_placeholder,
            scheduled: false,
            payload: revision.payload.clone(),
        };
        let (record, mut found) = convert_general_information(source);
        records.extend(record);
        issues.append(&mut found);
    }
}
