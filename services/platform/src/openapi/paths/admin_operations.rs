//! Administrator settings, operations, and audit path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;

/// Adds settings, background-operation, and audit endpoints.
pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/settings",
        "get",
        admin(
            op(
                "getPlatformSettings",
                "Get the allow-listed mutable business settings",
                "adminSettings",
                [(
                    "200",
                    response_header(
                        response_header(
                            json_response("Platform settings", r("PlatformSettings")),
                            "ETag",
                            "Current settings revision tag",
                        ),
                        "Cache-Control",
                        "private, no-store, max-age=0",
                    ),
                )],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/settings",
        "patch",
        admin(
            params(
                body(
                    op(
                        "updatePlatformSettings",
                        "Atomically update allow-listed mutable business settings",
                        "adminSettings",
                        [(
                            "200",
                            response_header(
                                response_header(
                                    json_response(
                                        "Platform settings updated",
                                        r("PlatformSettings"),
                                    ),
                                    "ETag",
                                    "New settings revision tag",
                                ),
                                "Cache-Control",
                                "private, no-store, max-age=0",
                            ),
                        )],
                    ),
                    r("UpdatePlatformSettings"),
                ),
                vec![if_match_param()],
            ),
            true,
        ),
    );

    add_admin_list(
        paths,
        "/api/admin/v1/operations",
        "listBackgroundOperations",
        "List predefined background operations",
        "BackgroundOperationPage",
    );
    let create_operation = params(
        body(
            op(
                "createBackgroundOperation",
                "Queue a predefined background operation",
                "adminOperations",
                [(
                    "202",
                    response_header(
                        json_response("Operation queued", r("BackgroundOperation")),
                        "Location",
                        "Operation status URL",
                    ),
                )],
            ),
            r("CreateOperationRequest"),
        ),
        vec![idempotency_param(), totp_param()],
    );
    add(
        paths,
        "/api/admin/v1/operations",
        "post",
        admin(create_operation, true),
    );
    add(
        paths,
        "/api/admin/v1/operations/{id}",
        "get",
        admin(
            params(
                op(
                    "getBackgroundOperation",
                    "Get background operation status",
                    "adminOperations",
                    [(
                        "200",
                        json_response("Background operation", r("BackgroundOperation")),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/operations/{id}/events",
        "get",
        admin(
            params(
                op(
                    "streamBackgroundOperationEvents",
                    "Stream operation status as a server-sent event",
                    "adminOperations",
                    [(
                        "200",
                        text_response(
                            "Operation event stream",
                            "text/event-stream",
                            json!({"type": "string"}),
                        ),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add_admin_list(
        paths,
        "/api/admin/v1/audit",
        "listAuditEvents",
        "List immutable audit events",
        "AuditEventPage",
    );
}
