//! Unified CMS content, draft, snapshot, revision, and diff path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;
use crate::error::CONTENT_DEPENDENCY_CONFLICT;

pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add_collection_paths(paths);
    add_draft_paths(paths);
    add_snapshot_path(paths);
    add_unpublish_path(paths);
    add_archive_path(paths);
    add_revision_paths(paths);
    add_media_paths(paths);
}

fn add_archive_path(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content/{id}/archive",
        "post",
        admin(
            params(
                body(
                    op(
                        "archiveAdminContentV2",
                        "Archive content only after it has been explicitly unpublished",
                        "adminContent",
                        [("200", content_response("Content archived"))],
                    ),
                    r("ArchiveContentRequest"),
                ),
                vec![
                    path_param("id", uuid()),
                    draft_if_match_param(),
                    idempotency_param(),
                ],
            ),
            true,
        ),
    );
}

fn add_unpublish_path(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content/{id}/unpublish",
        "post",
        with_problem_example(
            admin(
                params(
                    body(
                        op(
                            "unpublishAdminContentV2",
                            "Explicitly unpublish content while preserving its draft and immutable history",
                            "adminContent",
                            [("200", content_response("Content unpublished"))],
                        ),
                        r("UnpublishContentRequest"),
                    ),
                    vec![path_param("id", uuid()), draft_if_match_param(), idempotency_param()],
                ),
                true,
            ),
            "409",
            CONTENT_DEPENDENCY_CONFLICT,
            "Content dependency conflict",
            "Active published content still depends on this target.",
        ),
    );
}

fn add_media_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/media/assets",
        "get",
        admin(
            params(
                op(
                    "listAdminMediaAssets",
                    "List media library assets for the unified content editor",
                    "adminContent",
                    [("200", json_response("Media assets", r("MediaAssetPage")))],
                ),
                media_asset_params(),
            ),
            false,
        ),
    );
}

fn media_asset_params() -> Vec<Value> {
    let mut values = admin_pagination_params();
    values.push(query_param(
        "q",
        false,
        json!({"type": "string", "maxLength": 200, "description": "Case-insensitive original name search."}),
    ));
    values
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
                    "List unified CMS working records with server-side search, filters, sort, and type counts",
                    "adminContent",
                    [(
                        "200",
                        json_response("Content records", r("ContentRecordV2Page")),
                    )],
                ),
                content_list_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/templates",
        "get",
        admin(
            op(
                "listAdminContentTemplatesV2",
                "List the controlled CMS template registry",
                "adminContent",
                [(
                    "200",
                    json_response(
                        "Controlled content templates",
                        r("ContentTemplateDefinitionPage"),
                    ),
                )],
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

fn content_list_params() -> Vec<Value> {
    let mut values = admin_pagination_params();
    values.push(query_param(
        "q",
        false,
        json!({"type": "string", "maxLength": 200, "description": "Case-insensitive title or slug search."}),
    ));
    values.push(query_param(
        "kind",
        false,
        json!({"type": "string", "description": "Comma-separated CmsContentKind filters."}),
    ));
    values.push(query_param("status", false, r("CmsPublicationStatusV2")));
    values.push(query_param(
        "sort",
        false,
        json!({"type": "string", "enum": ["updatedAt", "title", "kind"], "default": "updatedAt"}),
    ));
    values.push(query_param(
        "direction",
        false,
        json!({"type": "string", "enum": ["asc", "desc"]}),
    ));
    values
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
        with_problem_example(
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
            "422",
            CONTENT_DEPENDENCY_CONFLICT,
            "Content dependency conflict",
            "One or more publication dependencies failed validation.",
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
