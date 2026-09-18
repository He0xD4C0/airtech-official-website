//! Administrator Feishu connector, scheduling, run, and diagnostic paths.

use serde_json::{Map, Value};

use super::super::support::*;

pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add_run_paths(paths);
    add_settings_paths(paths);
    add_diagnostic_paths(paths);
}

fn add_run_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/feishu/sync-runs",
        "get",
        admin(
            params(
                op(
                    "listFeishuSyncRuns",
                    "List automatic Feishu synchronization runs",
                    "adminFeishu",
                    [("200", json_response("Sync runs", r("SyncRunPage")))],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/feishu/sync-runs",
        "post",
        admin(
            body(
                op(
                    "startFeishuSyncRun",
                    "Queue an incremental or full Feishu synchronization",
                    "adminFeishu",
                    [(
                        "202",
                        response_header(
                            json_response("Sync run accepted", r("SyncRun")),
                            "Location",
                            "Sync run detail URL",
                        ),
                    )],
                ),
                r("StartSyncRequest"),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/feishu/sync-runs/{id}",
        "get",
        admin(
            params(
                op(
                    "getFeishuSyncRun",
                    "Read counters and isolated errors for one synchronization run",
                    "adminFeishu",
                    [(
                        "200",
                        json_response("Sync run detail", r("FeishuSyncRunDetail")),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/feishu/sync-runs/{id}/rollback",
        "post",
        admin(
            params(
                op(
                    "rollbackFeishuSyncRun",
                    "Restore prior public revisions when no later run changed them",
                    "adminFeishu",
                    [(
                        "200",
                        json_response("Rollback report", r("FeishuRollbackReport")),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            true,
        ),
    );
}

fn add_settings_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/feishu/settings",
        "get",
        admin(
            op(
                "getFeishuSettings",
                "Read non-secret Feishu sources and scheduling policy",
                "adminFeishu",
                [(
                    "200",
                    response_header(
                        json_response("Feishu settings", r("FeishuSettings")),
                        "ETag",
                        "Settings revision tag",
                    ),
                )],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/feishu/settings",
        "put",
        admin(
            params(
                body(
                    op(
                        "updateFeishuSettings",
                        "Update Feishu enablement and automatic schedule",
                        "adminFeishu",
                        [(
                            "200",
                            response_header(
                                json_response("Feishu settings", r("FeishuSettings")),
                                "ETag",
                                "Updated settings revision tag",
                            ),
                        )],
                    ),
                    r("UpdateFeishuSettings"),
                ),
                vec![if_match_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/feishu/connection-test",
        "post",
        admin(
            op(
                "testFeishuConnection",
                "Test credentials, all four tables, field mapping, and object storage",
                "adminFeishu",
                [(
                    "200",
                    json_response("Connection test", r("FeishuConnectionTest")),
                )],
            ),
            true,
        ),
    );
}

fn add_diagnostic_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/feishu/connection-status",
        "get",
        admin(
            op(
                "getFeishuConnectionStatus",
                "Read credential-safe Feishu connector status and latest run",
                "adminFeishu",
                [(
                    "200",
                    json_response("Feishu connection status", r("FeishuConnectionStatus")),
                )],
            ),
            false,
        ),
    );
    add_page_path(
        paths,
        "/api/admin/v1/feishu/mappings",
        "listFeishuMappings",
        "List versioned Feishu field mappings",
        "SyncMappingPage",
        vec![],
    );
    add_page_path(
        paths,
        "/api/admin/v1/feishu/staging",
        "listFeishuStagingRecords",
        "List Feishu staging records and validation results",
        "StagingRecordPage",
        vec![
            query_param("syncRunId", false, uuid()),
            query_param("status", false, r("StagingValidationStatus")),
            query_param(
                "q",
                false,
                serde_json::json!({"type": "string", "maxLength": 200}),
            ),
        ],
    );
    add_page_path(
        paths,
        "/api/admin/v1/feishu/conflicts",
        "listFeishuConflicts",
        "List legacy manually created source conflicts",
        "SyncConflictPage",
        vec![
            query_param(
                "q",
                false,
                serde_json::json!({"type": "string", "maxLength": 200}),
            ),
            query_param(
                "openOnly",
                false,
                serde_json::json!({"type": "boolean", "default": true}),
            ),
        ],
    );
    add(
        paths,
        "/api/admin/v1/feishu/conflicts/{id}/resolve",
        "post",
        admin(
            params(
                body(
                    op(
                        "resolveFeishuConflict",
                        "Resolve one legacy Feishu conflict",
                        "adminFeishu",
                        [(
                            "200",
                            json_response("Resolved sync conflict", r("SyncConflict")),
                        )],
                    ),
                    r("ResolveSyncConflictRequest"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
}

fn add_page_path(
    paths: &mut Map<String, Value>,
    path: &str,
    operation_id: &str,
    summary: &str,
    schema: &str,
    extra_parameters: Vec<Value>,
) {
    let mut parameters = admin_pagination_params();
    parameters.extend(extra_parameters);
    add(
        paths,
        path,
        "get",
        admin(
            params(
                op(
                    operation_id,
                    summary,
                    "adminFeishu",
                    [("200", json_response(summary, r(schema)))],
                ),
                parameters,
            ),
            false,
        ),
    );
}
