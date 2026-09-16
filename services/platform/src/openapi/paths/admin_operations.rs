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

    add(
        paths,
        "/api/admin/v1/operations/{id}",
        "get",
        admin(
            params(
                op(
                    "getBackgroundOperation",
                    "Get Product Master import operation status",
                    "adminProductImports",
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
                    "Stream Product Master import status as a server-sent event",
                    "adminProductImports",
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
    let audit_parameters = || {
        let mut parameters = admin_pagination_params();
        parameters.extend([
            query_param("actor", false, json!({"type": "string"})),
            query_param("action", false, json!({"type": "string"})),
            query_param("resourceType", false, json!({"type": "string"})),
            query_param("resourceId", false, uuid()),
            query_param("from", false, timestamp()),
            query_param("to", false, timestamp()),
            query_param("q", false, json!({"type": "string", "maxLength": 200})),
        ]);
        parameters
    };
    add(
        paths,
        "/api/admin/v1/audit",
        "get",
        admin(
            params(
                op(
                    "listAuditEvents",
                    "List immutable audit events with server-side actor, action, resource, time and full-text filters",
                    "adminAudit",
                    [("200", json_response("Audit events", r("AuditEventPage")))],
                ),
                audit_parameters(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/audit/export.csv",
        "get",
        admin(
            params(
                op(
                    "exportAuditEventsCsv",
                    "Export filtered audit events as ordinary unsigned CSV",
                    "adminAudit",
                    [(
                        "200",
                        text_response("Filtered audit CSV", "text/csv", json!({"type": "string"})),
                    )],
                ),
                audit_parameters(),
            ),
            false,
        ),
    );
}
