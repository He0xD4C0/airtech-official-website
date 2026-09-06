//! Unified CMS content, draft, snapshot, revision, and diff path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add_collection_paths(paths);
    add_draft_paths(paths);
    add_snapshot_path(paths);
    add_revision_paths(paths);
}

fn add_collection_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content",
        "get",
        admin(
            params(
                op(
                    "listAdminContentV2",
                    "List unified CMS working records",
                    "adminContent",
                    [(
                        "200",
                        json_response("Content records", r("ContentRecordV2Page")),
                    )],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content",
        "post",
        admin(
            params(
                body(
                    op(
                        "createAdminContentV2",
                        "Create a unified CMS draft without creating a revision",
                        "adminContent",
                        [("201", content_response("Content draft created"))],
                    ),
                    r("ContentDraftV2"),
                ),
                vec![draft_if_match_param(), idempotency_param()],
            ),
            true,
        ),
    );
}

fn add_draft_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content/{id}/draft",
        "get",
        admin(
            params(
                op(
                    "getAdminContentDraftV2",
                    "Get the current unified CMS draft",
                    "adminContent",
                    [("200", content_response("Current content draft"))],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/{id}/draft",
        "patch",
        admin(
            params(
                body(
                    op(
                        "updateAdminContentDraftV2",
                        "Auto-save the draft and increment only draftVersion",
                        "adminContent",
                        [("200", content_response("Content draft saved"))],
                    ),
                    r("ContentDraftV2"),
                ),
                mutation_params(false),
            ),
            true,
        ),
    );
}

fn add_snapshot_path(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content/{id}/snapshots",
        "post",
        admin(
            params(
                body(
                    op(
                        "createAdminContentSnapshotV2",
                        "Create an immutable manual or published revision from the current draft",
                        "adminContent",
                        [("201", content_response("Content snapshot created"))],
                    ),
                    r("CreateContentSnapshotRequest"),
                ),
                mutation_params(false),
            ),
            true,
        ),
    );
}

fn add_revision_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content/{id}/revisions",
        "get",
        admin(
            params(
                op(
                    "listAdminContentRevisionsV2",
                    "List immutable unified CMS revisions",
                    "adminContent",
                    [(
                        "200",
                        json_response("Content revisions", r("ContentRevisionV2Page")),
                    )],
                ),
                {
                    let mut values = vec![path_param("id", uuid())];
                    values.extend(admin_pagination_params());
                    values
                },
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/{id}/diff",
        "get",
        admin(
            params(
                op(
                    "getAdminContentDiffV2",
                    "Compare an immutable revision with another revision or the current draft",
                    "adminContent",
                    [("200", json_response("Content diff", r("ContentDiffV2")))],
                ),
                vec![
                    path_param("id", uuid()),
                    query_param("baseRevision", true, revision()),
                    query_param("targetRevision", false, revision()),
                ],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/{id}/revisions/{revision}/restore",
        "post",
        admin(
            params(
                body(
                    op(
                        "restoreAdminContentRevisionV2",
                        "Restore an immutable revision as a new draft and immutable restore revision",
                        "adminContent",
                        [("201", content_response("Content revision restored"))],
                    ),
                    r("RestoreContentRevisionRequest"),
                ),
                mutation_params(true),
            ),
            true,
        ),
    );
}

fn mutation_params(with_revision: bool) -> Vec<Value> {
    let mut values = vec![path_param("id", uuid())];
    if with_revision {
        values.push(path_param("revision", revision()));
    }
    values.push(draft_if_match_param());
    values.push(idempotency_param());
    values
}

fn draft_if_match_param() -> Value {
    json!({
        "name": "If-Match", "in": "header", "required": true,
        "description": "Current draft ETag. Creation requires draft-0.",
        "schema": {"type": "string", "pattern": "^\\\"draft-[0-9]+\\\"$"}
    })
}

fn content_response(description: &str) -> Value {
    response_header(
        json_response(description, r("ContentRecordV2")),
        "ETag",
        "Current draft-N entity tag",
    )
}
