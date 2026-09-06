use super::types::{CmsPreflightIssue, CmsPreflightSeverity, LegacySnapshot};

pub(super) fn sort_snapshot(snapshot: &mut LegacySnapshot) {
    snapshot.content_entries.sort_by_key(|value| value.id);
    snapshot
        .content_revisions
        .sort_by_key(|value| (value.content_id, value.revision));
    snapshot
        .news
        .sort_by_key(|value| (value.content_id, value.revision));
    snapshot.news_working.sort_by_key(|value| value.content_id);
    snapshot.general_information.sort_by_key(|value| value.id);
    snapshot
        .general_information_revisions
        .sort_by_key(|value| (value.general_information_id, value.revision));
    snapshot.content_relations.sort_by_key(|value| {
        (
            value.from_id,
            value.relation_type.clone(),
            value.sort_order,
            value.to_id,
        )
    });
    snapshot.media_assets.sort_by_key(|value| value.id);
    snapshot.asset_references.sort_by_key(|value| value.id);
    snapshot
        .public_routes
        .sort_by_key(|value| (value.canonical_path.clone(), value.id));
    snapshot.products.sort_by_key(|value| value.id);
}

pub(super) fn sort_issues(issues: &mut [CmsPreflightIssue]) {
    issues.sort_by_key(|value| {
        (
            if value.severity == CmsPreflightSeverity::Blocking {
                0
            } else {
                1
            },
            format!("{:?}", value.source),
            value.entity_id,
            value.revision,
            value.json_path.clone(),
            format!("{:?}", value.code),
            value.message.clone(),
        )
    });
}
