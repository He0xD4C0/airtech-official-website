//! Administrator content path definitions.

use serde_json::{Map, Value};

use super::super::support::*;

/// Adds generic content authoring and publication endpoints.
pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/content",
        "get",
        admin(
            params(
                op(
                    "listAdminContent",
                    "List non-News content working records; News uses the dedicated Admin News API",
                    "adminContent",
                    [("200", json_response("Content records", r("ContentPage")))],
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
                        "createContentDraft",
                        "Create a non-News content draft; kind=news is rejected in favor of the dedicated Admin News API",
                        "adminContent",
                        [(
                            "201",
                            response_header(
                                json_response("Draft created", r("ContentEntry")),
                                "ETag",
                                "Current working revision tag",
                            ),
                        )],
                    ),
                    r("ContentDraftInput"),
                ),
                vec![idempotency_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/{id}",
        "patch",
        admin(
            params(
                body(
                    entity_op(
                        "updateContentDraft",
                        "Update a non-News content draft; existing or requested News is rejected",
                        "Draft updated",
                    ),
                    r("ContentDraftInput"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/{id}/preview",
        "post",
        admin(
            params(
                body(
                    op(
                        "createContentPreview",
                        "Create a short-lived URL bound to the issuing Admin user and session for one immutable content revision",
                        "adminContent",
                        [(
                            "201",
                            response_header(
                                response_header(
                                    json_response("Preview URL created", r("ContentPreviewLink")),
                                    "Location",
                                    "Absolute short-lived Public Web preview URL",
                                ),
                                "Cache-Control",
                                "private, no-store, max-age=0",
                            ),
                        )],
                    ),
                    r("CreateContentPreviewRequest"),
                ),
                vec![path_param("id", uuid()), optional_if_match_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/{id}/publish",
        "post",
        admin(
            params(
                entity_op(
                    "publishContentRevision",
                    "Publish an immutable non-News content revision; News uses its dedicated publish operation",
                    "Content published",
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/content/{id}/rollback",
        "post",
        admin(
            params(
                body(
                    entity_op(
                        "rollbackContentRevision",
                        "Republish a historical non-News snapshot as a new immutable revision; News uses its dedicated rollback operation",
                        "Content republished",
                    ),
                    r("RollbackContentRequest"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
}
