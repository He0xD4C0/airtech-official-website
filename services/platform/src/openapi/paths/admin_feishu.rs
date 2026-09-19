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
            op(
                "startFeishuSyncRun",
                "Queue a full scan of every enabled Feishu source table",
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
}

fn add_settings_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/feishu/settings",
        "get",
        admin(
            op(
                "getFeishuSettings",
                "Read credential-safe Feishu connection, sources, and scheduling policy",
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
                        "Update GUI-managed Feishu credentials, sources, and automatic schedule",
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
                "Test credentials, enabled source tables, field mappings, and storage",
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
