//! Administrator product catalog and source-management path definitions.

use serde_json::{Map, Value};

use super::super::support::*;

fn product_list_params() -> Vec<Value> {
    let mut values = admin_pagination_params();
    values.push(query_param(
        "q",
        false,
        serde_json::json!({
            "type": "string",
            "maxLength": 200,
            "description": "Case-insensitive stable id, model, or title search."
        }),
    ));
    values
}

/// Adds product publication, temporary override, and Feishu sync endpoints.
pub(super) fn add_publication_and_sync(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/products",
        "get",
        admin(
            params(
                op(
                    "listAdminProducts",
                    "List product working records",
                    "adminCatalog",
                    [("200", json_response("Product records", r("ProductPage")))],
                ),
                product_list_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/{id}/publish",
        "post",
        admin(
            params(
                product_entity_op(
                    "publishProductRevision",
                    "Publish a validated product revision",
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/{id}/temporary-overrides",
        "get",
        admin(
            params(
                op(
                    "listTemporaryOverrides",
                    "List temporary source-field overrides",
                    "adminCatalog",
                    [(
                        "200",
                        json_response("Temporary overrides", r("TemporaryOverridePage")),
                    )],
                ),
                {
                    let mut parameters = vec![path_param("id", uuid())];
                    parameters.extend(admin_pagination_params());
                    parameters
                },
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/{id}/temporary-overrides",
        "post",
        admin(
            params(
                body(
                    op(
                        "createTemporaryOverride",
                        "Create an expiring source-field override",
                        "adminCatalog",
                        [(
                            "201",
                            json_response("Override created", r("TemporaryOverride")),
                        )],
                    ),
                    r("CreateTemporaryOverride"),
                ),
                vec![path_param("id", uuid()), idempotency_param()],
            ),
            true,
        ),
    );

    add(
        paths,
        "/api/admin/v1/feishu/sync-runs",
        "get",
        admin(
            params(
                op(
                    "listFeishuSyncRuns",
                    "List Feishu sync runs",
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
            params(
                body(
                    op(
                        "startFeishuSyncRun",
                        "Queue a resumable Feishu staging sync job",
                        "adminFeishu",
                        [("202", json_response("Sync run queued", r("SyncRun")))],
                    ),
                    r("StartSyncRequest"),
                ),
                vec![idempotency_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/feishu/conflicts",
        "get",
        admin(
            params(
                op(
                    "listFeishuConflicts",
                    "List three-way product conflicts",
                    "adminFeishu",
                    [(
                        "200",
                        json_response("Sync conflicts", r("SyncConflictPage")),
                    )],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
}

/// Adds Product Master import and product-presentation endpoints.
pub(super) fn add_management(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/products/imports",
        "get",
        admin(
            params(
                op(
                    "listProductImportRuns",
                    "List Product Master import reports",
                    "adminProductImports",
                    [(
                        "200",
                        json_response("Product import reports", r("ProductImportResultPage")),
                    )],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/imports",
        "post",
        admin(
            params(
                body(
                    op(
                        "importProductMaster",
                        "Validate and import an authoritative Product Master CSV",
                        "adminProductImports",
                        [(
                            "202",
                            response_header(
                                json_response(
                                    "Product import operation accepted",
                                    r("ProductImportAccepted"),
                                ),
                                "Location",
                                "Background operation status URL",
                            ),
                        )],
                    ),
                    r("ProductImportRequest"),
                ),
                vec![idempotency_param()],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/imports/{id}",
        "get",
        admin(
            params(
                op(
                    "getProductImportRun",
                    "Get one Product Master import report",
                    "adminProductImports",
                    [(
                        "200",
                        json_response("Product import report", r("ProductImportResult")),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/{id}",
        "get",
        admin(
            params(
                op(
                    "getAdminProduct",
                    "Get product facts, source authority and website presentation",
                    "adminCatalog",
                    [(
                        "200",
                        response_header(
                            json_response("Admin product detail", r("AdminProductDetail")),
                            "ETag",
                            "Current independent website presentation revision tag",
                        ),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/{id}/private-pricing",
        "get",
        admin(
            params(
                op(
                    "getProductPrivatePricing",
                    "Decrypt allowlisted Product Master pricing fields for an authorized user",
                    "adminCatalog",
                    [(
                        "200",
                        json_response("Private Product Master pricing", r("ProductPrivatePricing")),
                    )],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/products/{id}/presentation",
        "patch",
        admin(
            params(
                body(
                    op(
                        "updateProductPresentation",
                        "Update product website presentation and SEO fields",
                        "adminCatalog",
                        [(
                            "200",
                            response_header(
                                json_response(
                                    "Admin product detail updated",
                                    r("AdminProductDetail"),
                                ),
                                "ETag",
                                "New independent product presentation revision tag",
                            ),
                        )],
                    ),
                    r("UpdateProductPresentation"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
}
