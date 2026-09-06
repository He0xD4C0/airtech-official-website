//! Unified CMS service envelopes and mutation request schemas.

use serde_json::{json, Map, Value};

use super::super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    add_enums_and_requests(s);
    add_records(s);
    add_diff(s);
}

fn add_enums_and_requests(s: &mut Map<String, Value>) {
    s.insert(
        "ContentRevisionKindV2".into(),
        string_enum(&["manual", "publish", "restore"]),
    );
    s.insert(
        "ContentSnapshotIntent".into(),
        string_enum(&["manual", "publish"]),
    );
    s.insert(
        "CreateContentSnapshotRequest".into(),
        object(
            &["intent", "reason"],
            json!({
                "intent": r("ContentSnapshotIntent"),
                "reason": {"type": "string", "minLength": 10, "maxLength": 2000}
            }),
        ),
    );
    s.insert(
        "RestoreContentRevisionRequest".into(),
        object(
            &["reason"],
            json!({"reason": {"type": "string", "minLength": 10, "maxLength": 2000}}),
        ),
    );
}

fn add_records(s: &mut Map<String, Value>) {
    s.insert(
        "ContentRecordV2".into(),
        object(
            &[
                "id",
                "status",
                "draft",
                "latestRevision",
                "publishedRevision",
                "createdAt",
                "updatedAt",
                "updatedBy",
            ],
            json!({
                "id": uuid(),
                "status": r("CmsPublicationStatusV2"),
                "draft": r("ContentDraftV2"),
                "latestRevision": nullable(revision()),
                "publishedRevision": nullable(revision()),
                "createdAt": timestamp(),
                "updatedAt": timestamp(),
                "updatedBy": {"type": "string", "minLength": 1, "maxLength": 320}
            }),
        ),
    );
    s.insert(
        "ContentRevisionV2".into(),
        object(
            &[
                "contentId",
                "revision",
                "sourceDraftVersion",
                "kind",
                "document",
                "reason",
                "createdBy",
                "createdAt",
            ],
            json!({
                "contentId": uuid(),
                "revision": revision(),
                "sourceDraftVersion": revision(),
                "kind": r("ContentRevisionKindV2"),
                "document": r("ContentDraftV2"),
                "reason": {"type": "string", "minLength": 1, "maxLength": 2000},
                "createdBy": {"type": "string", "minLength": 1, "maxLength": 320},
                "createdAt": timestamp()
            }),
        ),
    );
    s.insert("ContentRecordV2Page".into(), page("ContentRecordV2"));
    s.insert("ContentRevisionV2Page".into(), page("ContentRevisionV2"));
}

fn add_diff(s: &mut Map<String, Value>) {
    s.insert(
        "ContentDiffChange".into(),
        object(
            &["path", "before", "after"],
            json!({
                "path": {"type": "string", "minLength": 1, "maxLength": 2000},
                "before": nullable(json!({})),
                "after": nullable(json!({}))
            }),
        ),
    );
    s.insert(
        "ContentDiffV2".into(),
        object(
            &[
                "contentId",
                "baseRevision",
                "targetRevision",
                "targetDraftVersion",
                "changes",
            ],
            json!({
                "contentId": uuid(),
                "baseRevision": revision(),
                "targetRevision": nullable(revision()),
                "targetDraftVersion": nullable(revision()),
                "changes": {"type": "array", "maxItems": 5000, "items": r("ContentDiffChange")}
            }),
        ),
    );
}
