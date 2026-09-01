use serde_json::{json, Map, Value};

/// Builds the contract from the same feature set as the compiled API.
/// The checked-in snapshot is exported with the `production` feature, which
/// makes development-only routes impossible to publish accidentally.
pub fn document() -> Value {
    let mut paths = Map::new();

    add(
        &mut paths,
        "/healthz",
        "get",
        op(
            "getLiveness",
            "Report process liveness",
            "system",
            [("200", json_response("Service is alive", r("HealthStatus")))],
        ),
    );
    add(
        &mut paths,
        "/readyz",
        "get",
        op(
            "getReadiness",
            "Report persistence readiness",
            "system",
            [("200", json_response("Service is ready", r("HealthStatus")))],
        ),
    );
    add(
        &mut paths,
        "/openapi.json",
        "get",
        op(
            "getOpenApiDocument",
            "Return the contract for this compiled API",
            "system",
            [(
                "200",
                json_response("OpenAPI document", r("OpenApiDocument")),
            )],
        ),
    );
    add(
        &mut paths,
        "/robots.txt",
        "get",
        op(
            "getApiRobots",
            "Disallow crawler access to the API origin",
            "system",
            [(
                "200",
                text_response("Crawler policy", "text/plain", json!({"type": "string"})),
            )],
        ),
    );

    add(
        &mut paths,
        "/api/public/v1/content/{kind}/{slug}",
        "get",
        params(
            op(
                "getPublishedContent",
                "Get published content",
                "public",
                [(
                    "200",
                    response_header(
                        json_response("Published content", r("ContentEntry")),
                        "ETag",
                        "Immutable published revision tag",
                    ),
                )],
            ),
            vec![
                path_param("kind", r("ContentKind")),
                path_param("slug", slug()),
                query_param(
                    "locale",
                    false,
                    json!({"type": "string", "enum": ["en"], "default": "en"}),
                ),
            ],
        ),
    );
    let mut preview_operation = op(
        "getContentPreview",
        "Get one exact content revision using a short-lived signed bearer token",
        "publicPreview",
        [(
            "200",
            json_response("Content preview", r("ContentPreviewResponse")),
        )],
    );
    preview_operation["security"] = json!([{"previewToken": []}]);
    preview_operation["responses"]["410"] =
        problem_response("The signed preview token has expired");
    add(
        &mut paths,
        "/api/public/v1/content-preview",
        "get",
        preview_operation,
    );
    add(
        &mut paths,
        "/api/public/v1/products",
        "get",
        params(
            op(
                "listPublishedProducts",
                "List published products",
                "publicCatalog",
                [("200", json_response("Published products", r("ProductPage")))],
            ),
            vec![
                query_param("family", false, r("ProductFamily")),
                query_param("motorTechnology", false, json!({"type": "string"})),
                json!({
                    "name": "cursor", "in": "query", "required": false,
                    "description": "Opaque base64url v1 keyset cursor bound to the active family and motorTechnology filters.",
                    "schema": {"type": "string", "minLength": 1, "maxLength": 2048, "pattern": "^[A-Za-z0-9_-]+$"}
                }),
                json!({
                    "name": "limit", "in": "query", "required": false,
                    "description": "Page size; values outside 1 through 100 return Problem Details 400.",
                    "schema": {"type": "integer", "minimum": 1, "maximum": 100, "default": 24}
                }),
            ],
        ),
    );
    add(
        &mut paths,
        "/api/public/v1/products/{slug}",
        "get",
        params(
            op(
                "getPublishedProduct",
                "Get a published product",
                "publicCatalog",
                [(
                    "200",
                    response_header(
                        json_response("Published product", r("Product")),
                        "ETag",
                        "Immutable published revision tag",
                    ),
                )],
            ),
            vec![path_param("slug", slug())],
        ),
    );
    add(
        &mut paths,
        "/api/public/v1/discovery",
        "get",
        op(
            "getPublicDiscovery",
            "List canonical indexable published URLs for sitemap generation",
            "publicDiscovery",
            [(
                "200",
                json_response("Public discovery feed", r("DiscoveryDocument")),
            )],
        ),
    );
    add(
        &mut paths,
        "/api/public/v1/selector",
        "post",
        body(
            op(
                "selectProducts",
                "Evaluate validated selector candidates",
                "publicSelector",
                [(
                    "200",
                    json_response("Selector evaluation", r("SelectorResponse")),
                )],
            ),
            r("SelectorRequest"),
        ),
    );
    add(
        &mut paths,
        "/api/public/v1/contact",
        "post",
        params(
            body(
                op(
                    "createContactRequest",
                    "Submit a contact request",
                    "publicSubmissions",
                    [
                        (
                            "200",
                            json_response("Idempotent replay", r("AcceptedResponse")),
                        ),
                        (
                            "201",
                            json_response("Contact accepted", r("AcceptedResponse")),
                        ),
                    ],
                ),
                r("CreateContactRequest"),
            ),
            vec![idempotency_param()],
        ),
    );
    add(
        &mut paths,
        "/api/public/v1/rfqs",
        "post",
        params(
            body(
                op(
                    "createRfqSubmission",
                    "Submit a structured RFQ",
                    "publicSubmissions",
                    [
                        (
                            "200",
                            json_response("Idempotent replay", r("AcceptedResponse")),
                        ),
                        ("201", json_response("RFQ accepted", r("AcceptedResponse"))),
                    ],
                ),
                r("CreateRfqRequest"),
            ),
            vec![idempotency_param()],
        ),
    );
    add(
        &mut paths,
        "/api/public/v1/analytics/consents",
        "post",
        body(
            op(
                "createAnalyticsConsent",
                "Record an anonymous analytics consent decision and issue a bounded receipt",
                "publicAnalytics",
                [(
                    "201",
                    json_response("Consent receipt", r("AnalyticsConsentReceipt")),
                )],
            ),
            r("CreateAnalyticsConsent"),
        ),
    );
    add(
        &mut paths,
        "/api/public/v1/analytics/events",
        "post",
        body(
            op(
                "createAnalyticsEvent",
                "Accept an allowlisted analytics event",
                "publicAnalytics",
                [(
                    "202",
                    json_response("Consent-aware event receipt", r("AnalyticsEventReceipt")),
                )],
            ),
            r("CreateAnalyticsEvent"),
        ),
    );

    add(
        &mut paths,
        "/api/admin/v1/auth/setup",
        "post",
        body(
            op(
                "setupInitialAdministrator",
                "Create the first administrator with a deployment bootstrap token",
                "adminAuth",
                [("201", session_response("Initial administrator created"))],
            ),
            r("SetupRequest"),
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/login",
        "post",
        body(
            op(
                "loginAdministrator",
                "Create an admin cookie session",
                "adminAuth",
                [("200", session_response("Authenticated admin session"))],
            ),
            r("LoginRequest"),
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/session",
        "get",
        admin(
            op(
                "getAdminSession",
                "Get the current admin session and rotate CSRF",
                "adminAuth",
                [("200", session_response("Current admin session"))],
            ),
            false,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/logout",
        "post",
        admin(
            op(
                "logoutAdministrator",
                "Revoke the current admin session",
                "adminAuth",
                [("204", empty_response("Session revoked"))],
            ),
            true,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/totp/enrollment",
        "post",
        admin(
            op(
                "startTotpEnrollment",
                "Create or replace an unconfirmed encrypted TOTP enrollment secret",
                "adminAuth",
                [(
                    "200",
                    json_response("TOTP enrollment details", r("TotpEnrollment")),
                )],
            ),
            true,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/totp/confirm",
        "post",
        admin(
            body(
                op(
                    "confirmTotpEnrollment",
                    "Verify enrollment and return a one-time recovery-code set",
                    "adminAuth",
                    [(
                        "200",
                        json_response("One-time recovery codes", r("RecoveryCodeSet")),
                    )],
                ),
                r("TotpCodeRequest"),
            ),
            true,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/recovery-codes/regenerate",
        "post",
        admin(
            body(
                op(
                    "regenerateRecoveryCodes",
                    "Invalidate prior recovery codes and return a new one-time set",
                    "adminAuth",
                    [(
                        "200",
                        json_response("Replacement recovery codes", r("RecoveryCodeSet")),
                    )],
                ),
                r("TotpCodeRequest"),
            ),
            true,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/sessions",
        "get",
        admin(
            op(
                "listAdminSessions",
                "List the current administrator's active sessions",
                "adminAuth",
                [(
                    "200",
                    json_response("Active sessions", array(r("AdminSession"))),
                )],
            ),
            false,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/auth/sessions/{id}",
        "delete",
        admin(
            params(
                op(
                    "revokeAdminSession",
                    "Revoke one active session owned by the current administrator",
                    "adminAuth",
                    [("204", empty_response("Session revoked"))],
                ),
                vec![path_param("id", uuid())],
            ),
            true,
        ),
    );

    add(
        &mut paths,
        "/api/admin/v1/content",
        "get",
        admin(
            params(
                op(
                    "listAdminContent",
                    "List content working records",
                    "adminContent",
                    [("200", json_response("Content records", r("ContentPage")))],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/content",
        "post",
        admin(
            params(
                body(
                    op(
                        "createContentDraft",
                        "Create a content draft",
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
        &mut paths,
        "/api/admin/v1/content/{id}",
        "patch",
        admin(
            params(
                body(
                    entity_op(
                        "updateContentDraft",
                        "Update a content draft",
                        "Draft updated",
                    ),
                    r("ContentDraftInput"),
                ),
                entity_params(),
            ),
            true,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/content/{id}/preview",
        "post",
        admin(
            params(
                body(
                    op(
                        "createContentPreview",
                        "Create a short-lived URL for one immutable content revision",
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
        &mut paths,
        "/api/admin/v1/content/{id}/publish",
        "post",
        admin(
            params(
                entity_op(
                    "publishContentRevision",
                    "Publish an immutable content revision",
                    "Content published",
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );
    add(
        &mut paths,
        "/api/admin/v1/content/{id}/rollback",
        "post",
        admin(
            params(
                body(
                    entity_op(
                        "rollbackContentRevision",
                        "Republish a historical snapshot as a new immutable revision",
                        "Content republished",
                    ),
                    r("RollbackContentRequest"),
                ),
                idempotent_entity_params(),
            ),
            true,
        ),
    );

    add(
        &mut paths,
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
                admin_pagination_params(),
            ),
            false,
        ),
    );
    add(
        &mut paths,
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
        &mut paths,
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
        &mut paths,
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
        &mut paths,
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
        &mut paths,
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
        &mut paths,
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
    add_admin_list(
        &mut paths,
        "/api/admin/v1/rfqs",
        "listRfqSubmissions",
        "List RFQs with permission-aware PII redaction",
        "RfqSubmissionPage",
    );
    add_admin_list(
        &mut paths,
        "/api/admin/v1/contacts",
        "listContactRequests",
        "List contacts with permission-aware PII redaction",
        "ContactRequestPage",
    );
    add(
        &mut paths,
        "/api/admin/v1/analytics/summary",
        "get",
        admin(
            op(
                "getAnalyticsSummary",
                "Get first-party analytics totals",
                "adminAnalytics",
                [(
                    "200",
                    json_response("Analytics summary", r("AnalyticsSummary")),
                )],
            ),
            false,
        ),
    );

    add(
        &mut paths,
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
        &mut paths,
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
        &mut paths,
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
        &mut paths,
        "/api/admin/v1/operations",
        "post",
        admin(create_operation, true),
    );
    add(
        &mut paths,
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
        &mut paths,
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
        &mut paths,
        "/api/admin/v1/audit",
        "listAuditEvents",
        "List immutable audit events",
        "AuditEventPage",
    );

    #[cfg(feature = "devtools")]
    add_devtools_paths(&mut paths);

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "AIRTEKPOWER Platform API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Public and admin API contract. Exact product facts require a validated Feishu source snapshot."
        },
        "servers": [{"url": "http://localhost:8080", "description": "Fixed local application port"}],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "adminSession": {"type": "apiKey", "in": "cookie", "name": "airtek_admin_session", "description": "Host-only HttpOnly cookie scoped to /api."},
                "csrfToken": {"type": "apiKey", "in": "header", "name": "X-CSRF-Token", "description": "Required with the admin session on protected mutations."},
                "previewToken": {"type": "http", "scheme": "bearer", "bearerFormat": "AIRTEK preview v1", "description": "Short-lived signed capability for exactly one content revision. Never send it in a query parameter to the API."}
            },
            "schemas": schemas()
        },
        "x-airtek-conventions": {
            "jsonNaming": "camelCase",
            "timestamps": "UTC RFC3339",
            "identifiers": "UUID",
            "pagination": "cursor",
            "concurrency": "ETag and If-Match",
            "replaySafety": "Idempotency-Key",
            "longRunning": "202 operationId"
        },
        "x-airtek-build": if cfg!(feature = "devtools") { "development-with-devtools" } else if cfg!(feature = "production") { "production" } else { "development" }
    })
}

fn schemas() -> Value {
    let mut schemas = Map::new();
    add_common_schemas(&mut schemas);
    add_content_schemas(&mut schemas);
    add_product_schemas(&mut schemas);
    add_submission_schemas(&mut schemas);
    add_admin_schemas(&mut schemas);
    Value::Object(schemas)
}

// Schema groups are kept near their Rust domain counterparts and deliberately
// explicit so openapi-typescript can produce useful application types.
fn add_common_schemas(s: &mut Map<String, Value>) {
    s.insert(
        "OpenApiDocument".into(),
        json!({"type": "object", "additionalProperties": true}),
    );
    s.insert("ProblemDetails".into(), json!({
        "type": "object", "additionalProperties": false,
        "required": ["type", "title", "status", "detail", "requestId"],
        "properties": {
            "type": {"type": "string", "format": "uri"}, "title": {"type": "string"},
            "status": {"type": "integer", "minimum": 400, "maximum": 599}, "detail": {"type": "string"},
            "instance": nullable(json!({"type": "string"})), "requestId": uuid(),
            "errors": {"type": "object", "additionalProperties": {"type": "array", "items": {"type": "string"}}}
        }
    }));
    s.insert("HealthStatus".into(), object(
        &["status", "service", "version", "persistence", "timestamp"],
        json!({"status": {"type": "string"}, "service": {"type": "string"}, "version": {"type": "string"}, "persistence": {"type": "string"}, "timestamp": timestamp()})
    ));
}

fn add_content_schemas(s: &mut Map<String, Value>) {
    s.insert(
        "ContentKind".into(),
        string_enum(&[
            "home",
            "solution",
            "technology",
            "article",
            "faq",
            "caseStudy",
            "download",
            "company",
            "legal",
            "navigation",
            "footer",
        ]),
    );
    s.insert(
        "PublicationStatus".into(),
        string_enum(&["draft", "scheduled", "published", "archived"]),
    );
    s.insert(
        "RichTextDocument".into(),
        object(
            &["schemaVersion", "doc"],
            json!({"schemaVersion": {"type": "integer", "minimum": 1}, "doc": {}}),
        ),
    );
    s.insert(
        "SeoMetadata".into(),
        object(
            &["title", "description", "canonicalPath", "indexable"],
            seo_properties(),
        ),
    );
    s.insert(
        "SeoMetadataInput".into(),
        json!({"type": "object", "additionalProperties": false, "required": ["indexable"], "properties": seo_properties()}),
    );
    s.insert("ContentEntry".into(), object(
        &["id", "kind", "slug", "locale", "title", "summary", "body", "seo", "status", "isPlaceholder", "currentRevision", "publishedRevision", "scheduledFor", "updatedAt"],
        json!({
            "id": uuid(), "kind": r("ContentKind"), "slug": slug(), "locale": {"type": "string"}, "title": {"type": "string"},
            "summary": nullable(json!({"type": "string"})), "body": r("RichTextDocument"), "seo": r("SeoMetadata"), "status": r("PublicationStatus"),
            "isPlaceholder": {"type": "boolean"}, "currentRevision": revision(), "publishedRevision": nullable(revision()),
            "scheduledFor": nullable(timestamp()), "updatedAt": timestamp()
        })
    ));
    s.insert("ContentDraftInput".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["kind", "slug", "title", "body"],
        "properties": {
            "kind": r("ContentKind"), "slug": slug(), "locale": {"type": "string", "enum": ["en"], "default": "en"},
            "title": {"type": "string", "minLength": 1, "maxLength": 300}, "summary": nullable(json!({"type": "string"})),
            "body": r("RichTextDocument"), "seo": r("SeoMetadataInput"), "isPlaceholder": {"type": "boolean", "default": false}
        }
    }));
    s.insert(
        "CreateContentPreviewRequest".into(),
        json!({
            "type": "object", "additionalProperties": false, "required": ["revision"],
            "properties": {
                "revision": revision(),
                "expiresInSeconds": {"type": "integer", "minimum": 1, "maximum": 600, "default": 600}
            }
        }),
    );
    s.insert(
        "ContentPreviewLink".into(),
        object(
            &["url", "contentId", "revision", "issuedAt", "expiresAt"],
            json!({
                "url": {"type": "string", "format": "uri", "readOnly": true},
                "contentId": uuid(), "revision": revision(),
                "issuedAt": timestamp(), "expiresAt": timestamp()
            }),
        ),
    );
    s.insert(
        "ContentPreviewResponse".into(),
        object(
            &["content", "previewExpiresAt"],
            json!({"content": r("ContentEntry"), "previewExpiresAt": timestamp()}),
        ),
    );
    s.insert(
        "RollbackContentRequest".into(),
        object(
            &["revision", "reason"],
            json!({"revision": revision(), "reason": {"type": "string", "minLength": 10}}),
        ),
    );
    s.insert("ContentPage".into(), page("ContentEntry"));
}

fn add_product_schemas(s: &mut Map<String, Value>) {
    s.insert(
        "FactState".into(),
        string_enum(&[
            "verified",
            "missing",
            "notApplicable",
            "notTested",
            "confidential",
            "pendingVerification",
        ]),
    );
    s.insert(
        "ProductFamily".into(),
        string_enum(&["centrifugal", "axial", "crossFlow", "inlineDuct", "motors"]),
    );
    s.insert("SpecValue".into(), object(
        &["key", "label", "value", "unit", "operatingCondition", "state", "sourceReference"],
        json!({"key": {"type": "string"}, "label": {"type": "string"}, "value": nullable(json!({})), "unit": nullable(json!({"type": "string"})), "operatingCondition": nullable(json!({"type": "string"})), "state": r("FactState"), "sourceReference": nullable(json!({"type": "string"}))})
    ));
    s.insert(
        "PerformancePoint".into(),
        object(
            &["airflow", "pressure"],
            json!({"airflow": {"type": "number"}, "pressure": {"type": "number"}}),
        ),
    );
    s.insert("PerformanceCurve".into(), object(
        &["airflowUnit", "pressureUnit", "speedRpm", "densityKgM3", "voltage", "testMethod", "sourceReference", "state", "points"],
        json!({
            "airflowUnit": {"type": "string"}, "pressureUnit": {"type": "string"}, "speedRpm": nullable(json!({"type": "integer", "minimum": 0})),
            "densityKgM3": nullable(json!({"type": "number"})), "voltage": nullable(json!({"type": "string"})), "testMethod": nullable(json!({"type": "string"})),
            "sourceReference": {"type": "string", "minLength": 1}, "state": r("FactState"), "points": array(r("PerformancePoint"))
        })
    ));
    s.insert("Product".into(), object(
        &["id", "stableId", "model", "slug", "locale", "family", "subtype", "motorTechnology", "title", "summary", "specifications", "performanceCurves", "sourceSnapshotId", "sourceRevision", "currentRevision", "publishedRevision", "status", "indexable", "updatedAt"],
        json!({
            "id": uuid(), "stableId": {"type": "string"}, "model": nullable(json!({"type": "string"})), "slug": slug(), "locale": {"type": "string"},
            "family": r("ProductFamily"), "subtype": nullable(json!({"type": "string"})), "motorTechnology": nullable(json!({"type": "string"})),
            "title": {"type": "string"}, "summary": nullable(json!({"type": "string"})), "specifications": array(r("SpecValue")), "performanceCurves": array(r("PerformanceCurve")),
            "sourceSnapshotId": uuid(), "sourceRevision": {"type": "string"}, "currentRevision": revision(), "publishedRevision": nullable(revision()),
            "status": r("PublicationStatus"), "indexable": {"type": "boolean"}, "updatedAt": timestamp()
        })
    ));
    s.insert("ProductPage".into(), product_page());
    s.insert(
        "SelectorPriority".into(),
        string_enum(&["efficiency", "noise", "size", "headroom"]),
    );
    s.insert("SelectorRequest".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["airflow", "airflowUnit", "pressure", "pressureUnit", "requiredCertifications"],
        "properties": {
            "airflow": {"type": "number", "exclusiveMinimum": 0}, "airflowUnit": {"type": "string"}, "pressure": {"type": "number", "exclusiveMinimum": 0}, "pressureUnit": {"type": "string"},
            "ambientTemperatureC": nullable(json!({"type": "number"})), "maximumDiameterMm": nullable(json!({"type": "number", "exclusiveMinimum": 0})),
            "voltage": nullable(json!({"type": "string"})), "frequencyHz": nullable(json!({"type": "number", "exclusiveMinimum": 0})),
            "requiredCertifications": array(json!({"type": "string"})), "preferredFamily": nullable(r("ProductFamily")), "priority": nullable(r("SelectorPriority"))
        }
    }));
    s.insert("SelectorCandidate".into(), object(
        &["productId", "productRevision", "title", "matchedConstraints", "warnings", "rank"],
        json!({"productId": uuid(), "productRevision": revision(), "title": {"type": "string"}, "matchedConstraints": array(json!({"type": "string"})), "warnings": array(json!({"type": "string"})), "rank": {"type": "integer", "minimum": 0}})
    ));
    s.insert(
        "SelectorOutcome".into(),
        string_enum(&[
            "matched",
            "noValidatedCandidates",
            "engineeringReviewRequired",
        ]),
    );
    s.insert("SelectorResponse".into(), object(&["outcome", "candidates", "explanations"], json!({"outcome": r("SelectorOutcome"), "candidates": array(r("SelectorCandidate")), "explanations": array(json!({"type": "string"}))})));
}

fn add_submission_schemas(s: &mut Map<String, Value>) {
    s.insert(
        "RfqJourney".into(),
        string_enum(&["product", "selection", "project", "replacement"]),
    );
    s.insert("BusinessContact".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["name", "email"],
        "properties": {
            "name": {"type": "string", "minLength": 1, "maxLength": 200}, "email": {"type": "string", "format": "email", "maxLength": 320},
            "phone": nullable(json!({"type": "string", "minLength": 1, "maxLength": 50})), "company": nullable(json!({"type": "string", "minLength": 1, "maxLength": 200})),
            "countryOrRegion": nullable(json!({"type": "string", "minLength": 1, "maxLength": 120}))
        }
    }));
    s.insert("ProductContext".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["productId", "stableId", "publishedRevision"],
        "properties": {"productId": uuid(), "stableId": {"type": "string", "minLength": 1, "maxLength": 200}, "model": nullable(json!({"type": "string", "minLength": 1, "maxLength": 200})), "publishedRevision": revision()}
    }));
    s.insert("ProductRfqProductContext".into(), object(
        &["productId", "stableId", "model", "publishedRevision"],
        json!({
            "productId": uuid(), "stableId": {"type": "string", "minLength": 1, "maxLength": 200},
            "model": {"type": "string", "minLength": 1, "maxLength": 200}, "publishedRevision": revision()
        })
    ));
    s.insert(
        "RfqQuantity".into(),
        json!({
            "oneOf": [
                {"type": "integer", "minimum": 1, "maximum": 1000000},
                {"type": "string", "pattern": "^[0-9]{1,7}$"}
            ]
        }),
    );
    s.insert(
        "RfqDutyPoint".into(),
        object(
            &["airflow", "airflowUnit", "pressure", "pressureUnit"],
            json!({
                "airflow": {"type": "number", "exclusiveMinimum": 0, "maximum": 1000000000},
                "airflowUnit": string_enum(&["m3/h", "m³/h", "CFM", "cfm"]),
                "pressure": {"type": "number", "exclusiveMinimum": 0, "maximum": 100000000},
                "pressureUnit": string_enum(&["Pa", "pa", "kPa", "kpa", "inH2O", "inh2o"])
            }),
        ),
    );
    s.insert(
        "RfqElectricalContext".into(),
        json!({
            "type": "object", "additionalProperties": false, "minProperties": 1,
            "properties": {
                "voltage": {"type": "string", "minLength": 1, "maxLength": 40},
                "frequencyHz": {"type": "number", "exclusiveMinimum": 0, "maximum": 1000}
            }
        }),
    );
    s.insert(
        "ProductRfqContext".into(),
        object(&["application"], Value::Object(rfq_context_properties([]))),
    );
    s.insert(
        "SelectionRfqContext".into(),
        object(
            &["application", "dutyPoint"],
            Value::Object(rfq_context_properties([
                ("dutyPoint", r("RfqDutyPoint")),
                ("maximumDiameterMm", json!({"type": "number", "exclusiveMinimum": 0, "maximum": 100000})),
                ("requiredCertifications", json!({"type": "array", "maxItems": 20, "items": {"type": "string", "minLength": 1, "maxLength": 120}})),
                ("control", json!({"type": "string", "minLength": 1, "maxLength": 120})),
            ])),
        ),
    );
    s.insert(
        "ProjectRfqContext".into(),
        object(
            &["application", "projectStage"],
            Value::Object(rfq_context_properties([
                (
                    "projectStage",
                    string_enum(&["Concept", "Engineering", "Prototype", "Production planning"]),
                ),
                (
                    "projectScale",
                    json!({"type": "string", "minLength": 1, "maxLength": 500}),
                ),
                (
                    "schedule",
                    json!({"type": "string", "minLength": 1, "maxLength": 500}),
                ),
                (
                    "engineeringNeeds",
                    json!({"type": "string", "minLength": 1, "maxLength": 4000}),
                ),
            ])),
        ),
    );
    s.insert(
        "ReplacementRfqContext".into(),
        object(
            &["application", "existingModel", "dutyPoint"],
            Value::Object(rfq_context_properties([
                (
                    "existingModel",
                    json!({"type": "string", "minLength": 1, "maxLength": 300}),
                ),
                ("dutyPoint", r("RfqDutyPoint")),
                (
                    "installationConstraints",
                    json!({"type": "string", "minLength": 1, "maxLength": 4000}),
                ),
                (
                    "replacementGoal",
                    json!({"type": "string", "minLength": 1, "maxLength": 2000}),
                ),
            ])),
        ),
    );
    s.insert(
        "ProductRfqRequest".into(),
        rfq_request_schema("product", "ProductRfqContext", true),
    );
    s.insert(
        "SelectionRfqRequest".into(),
        rfq_request_schema("selection", "SelectionRfqContext", false),
    );
    s.insert(
        "ProjectRfqRequest".into(),
        rfq_request_schema("project", "ProjectRfqContext", false),
    );
    s.insert(
        "ReplacementRfqRequest".into(),
        rfq_request_schema("replacement", "ReplacementRfqContext", false),
    );
    s.insert("CreateRfqRequest".into(), json!({
        "oneOf": [r("ProductRfqRequest"), r("SelectionRfqRequest"), r("ProjectRfqRequest"), r("ReplacementRfqRequest")],
        "discriminator": {
            "propertyName": "journey",
            "mapping": {
                "product": "#/components/schemas/ProductRfqRequest",
                "selection": "#/components/schemas/SelectionRfqRequest",
                "project": "#/components/schemas/ProjectRfqRequest",
                "replacement": "#/components/schemas/ReplacementRfqRequest"
            }
        }
    }));
    s.insert("CreateContactRequest".into(), object(
        &["contact", "topic", "message", "sourcePath", "locale", "consent"],
        json!({"contact": r("BusinessContact"), "topic": {"type": "string"}, "message": {"type": "string"}, "sourcePath": {"type": "string"}, "locale": {"type": "string"}, "consent": {"type": "boolean"}})
    ));
    s.insert(
        "AcceptedResponse".into(),
        object(
            &["id", "reference", "acceptedAt"],
            json!({"id": uuid(), "reference": {"type": "string"}, "acceptedAt": timestamp()}),
        ),
    );
    s.insert("RfqSubmission".into(), object(
        &["id", "reference", "request", "status", "submittedAt", "retentionUntil"],
        json!({"id": uuid(), "reference": {"type": "string"}, "request": r("CreateRfqRequest"), "status": {"type": "string"}, "submittedAt": timestamp(), "retentionUntil": timestamp()})
    ));
    s.insert("ContactRequest".into(), object(
        &["id", "reference", "request", "status", "submittedAt", "retentionUntil"],
        json!({"id": uuid(), "reference": {"type": "string"}, "request": r("CreateContactRequest"), "status": {"type": "string"}, "submittedAt": timestamp(), "retentionUntil": timestamp()})
    ));
    s.insert("RfqSubmissionPage".into(), page("RfqSubmission"));
    s.insert("ContactRequestPage".into(), page("ContactRequest"));
    s.insert(
        "AnalyticsPolicyVersion".into(),
        string_enum(&["analytics-v1"]),
    );
    s.insert(
        "CreateAnalyticsConsent".into(),
        object(
            &["anonymousSessionId", "policyVersion", "analyticsAllowed"],
            json!({
                "anonymousSessionId": uuid(), "policyVersion": r("AnalyticsPolicyVersion"),
                "analyticsAllowed": {"type": "boolean"}
            }),
        ),
    );
    s.insert("AnalyticsConsentReceipt".into(), object(
        &["consentReceipt", "anonymousSessionId", "policyVersion", "analyticsAllowed", "grantedAt", "expiresAt"],
        json!({
            "consentReceipt": uuid(), "anonymousSessionId": uuid(), "policyVersion": r("AnalyticsPolicyVersion"),
            "analyticsAllowed": {"type": "boolean"}, "grantedAt": timestamp(), "expiresAt": timestamp()
        })
    ));
    s.insert(
        "AnalyticsEventName".into(),
        string_enum(&[
            "pageView",
            "internalSearch",
            "filterApplied",
            "selectorStarted",
            "selectorStepCompleted",
            "selectorResult",
            "compareChanged",
            "downloadStarted",
            "faqExpanded",
            "ctaClicked",
            "rfqRouteSelected",
            "rfqStarted",
            "rfqStepCompleted",
            "rfqValidationError",
            "rfqSubmitted",
            "rfqSubmitFailed",
        ]),
    );
    s.insert(
        "AnalyticsEventProperties".into(),
        analytics_properties_schema(),
    );
    s.insert("ConsentedAnalyticsEvent".into(), object(
        &["eventName", "anonymousSessionId", "sourcePath", "locale", "consentGranted", "policyVersion", "consentReceipt"],
        json!({
            "eventName": r("AnalyticsEventName"), "anonymousSessionId": uuid(),
            "sourcePath": {"type": "string", "minLength": 3, "maxLength": 2048, "pattern": "^/en(?:/|$)[^?#]*$"},
            "locale": string_enum(&["en"]), "consentGranted": {"type": "boolean", "enum": [true]},
            "policyVersion": r("AnalyticsPolicyVersion"), "consentReceipt": uuid(),
            "properties": r("AnalyticsEventProperties")
        })
    ));
    s.insert("AnalyticsOptOutEvent".into(), object(
        &["eventName", "sourcePath", "locale", "consentGranted"],
        json!({
            "eventName": r("AnalyticsEventName"),
            "sourcePath": {"type": "string", "minLength": 3, "maxLength": 2048, "pattern": "^/en(?:/|$)[^?#]*$"},
            "locale": string_enum(&["en"]), "consentGranted": {"type": "boolean", "enum": [false]},
            "properties": {"type": "object", "maxProperties": 0, "additionalProperties": false, "default": {}}
        })
    ));
    s.insert(
        "CreateAnalyticsEvent".into(),
        json!({
            "oneOf": [r("ConsentedAnalyticsEvent"), r("AnalyticsOptOutEvent")]
        }),
    );
    s.insert(
        "AnalyticsEventReceipt".into(),
        object(
            &["accepted", "eventId"],
            json!({"accepted": {"type": "boolean"}, "eventId": nullable(uuid())}),
        ),
    );
    s.insert("DiscoveryEntry".into(), object(
        &["entityType", "entityId", "path", "locale", "title", "summary", "updatedAt"],
        json!({
            "entityType": string_enum(&["content", "product"]), "entityId": uuid(), "path": {"type": "string", "pattern": "^/en(?:/|$)"},
            "locale": {"type": "string"}, "title": {"type": "string"}, "summary": nullable(json!({"type": "string"})), "updatedAt": timestamp()
        })
    ));
    s.insert(
        "DiscoveryDocument".into(),
        object(
            &["generatedAt", "entries"],
            json!({"generatedAt": timestamp(), "entries": array(r("DiscoveryEntry"))}),
        ),
    );
}

fn rfq_context_properties<const N: usize>(extra: [(&str, Value); N]) -> Map<String, Value> {
    let mut properties = json!({
        "application": {"type": "string", "minLength": 1, "maxLength": 500},
        "quantity": r("RfqQuantity"),
        "electrical": r("RfqElectricalContext"),
        "environment": {"type": "string", "minLength": 1, "maxLength": 4000},
        "priority": string_enum(&["efficiency", "noise", "size", "headroom"]),
        "additionalMessage": {"type": "string", "minLength": 1, "maxLength": 10000}
    })
    .as_object()
    .expect("RFQ properties object")
    .clone();
    properties.extend(
        extra
            .into_iter()
            .map(|(name, schema)| (name.to_owned(), schema)),
    );
    properties
}

fn rfq_request_schema(journey: &str, context: &str, product_context: bool) -> Value {
    let mut required = vec![
        "journey",
        "contact",
        "sourcePath",
        "locale",
        "consent",
        "context",
    ];
    let mut properties = json!({
        "journey": {"type": "string", "enum": [journey]},
        "contact": r("BusinessContact"),
        "sourcePath": {"type": "string", "minLength": 3, "maxLength": 2048, "pattern": "^/en/request-a-quote(?:/|$)[^?#]*$"},
        "locale": string_enum(&["en"]),
        "consent": {"type": "boolean", "enum": [true]},
        "context": r(context)
    })
    .as_object()
    .expect("RFQ request properties")
    .clone();
    if product_context {
        required.push("productContext");
        properties.insert("productContext".into(), r("ProductRfqProductContext"));
    }
    object(&required, Value::Object(properties))
}

fn analytics_properties_schema() -> Value {
    let identifier = || json!({"type": "string", "minLength": 1, "maxLength": 120, "pattern": "^[A-Za-z0-9][A-Za-z0-9._:-]*$"});
    let controlled_text = || json!({"type": "string", "minLength": 1, "maxLength": 160});
    let bounded_count = || json!({"type": "integer", "minimum": 0, "maximum": 1000000});
    json!({
        "description": "A scalar-only property object. The server selects the matching strict allowlist from eventName and rejects every unknown key or PII-like value.",
        "anyOf": [
            object(&[], json!({"contentKind": identifier(), "contentId": uuid(), "publishedRevision": revision()})),
            object(&[], json!({"queryLength": {"type": "integer", "minimum": 1, "maximum": 500}, "resultCount": bounded_count()})),
            object(&[], json!({"filterName": identifier(), "filterValue": controlled_text(), "resultCount": bounded_count()})),
            object(&[], json!({"constraintCount": bounded_count(), "preferredFamily": string_enum(&["open", "centrifugal", "axial", "crossFlow", "inlineDuct", "motors"]), "priority": string_enum(&["efficiency", "noise", "size", "headroom"])})),
            object(&[], json!({"step": {"type": "integer", "minimum": 1, "maximum": 20}, "constraintCount": bounded_count()})),
            object(&[], json!({"outcome": string_enum(&["matched", "noValidatedCandidates", "engineeringReviewRequired"]), "candidateCount": bounded_count()})),
            object(&[], json!({"action": string_enum(&["add", "remove", "clear"]), "itemCount": {"type": "integer", "minimum": 0, "maximum": 4}, "productId": uuid(), "productRevision": revision()})),
            object(&[], json!({"downloadId": uuid(), "productId": uuid(), "productRevision": revision()})),
            object(&[], json!({"faqId": identifier(), "category": controlled_text()})),
            object(&[], json!({"ctaId": identifier(), "destinationPath": {"type": "string", "minLength": 3, "maxLength": 512, "pattern": "^/en(?:/|$)[^?#]*$"}, "placement": identifier()})),
            object(&[], json!({"journey": string_enum(&["product", "selection", "project", "replacement"]), "productId": uuid(), "productRevision": revision(), "step": {"type": "integer", "minimum": 1, "maximum": 20}, "fieldName": identifier(), "errorCode": identifier()}))
        ]
    })
}

fn add_admin_schemas(s: &mut Map<String, Value>) {
    s.insert("SetupRequest".into(), object(
        &["displayName", "email", "password", "bootstrapToken"],
        json!({"displayName": {"type": "string", "minLength": 1, "maxLength": 120}, "email": {"type": "string", "format": "email"}, "password": {"type": "string", "format": "password", "minLength": 12, "writeOnly": true}, "bootstrapToken": {"type": "string", "writeOnly": true}})
    ));
    s.insert("LoginRequest".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["email", "password"],
        "properties": {"email": {"type": "string", "format": "email"}, "password": {"type": "string", "format": "password", "writeOnly": true}, "otp": nullable(json!({"type": "string", "description": "A six-digit TOTP or one unused recovery code.", "pattern": "^(?:[0-9]{6}|[A-HJ-NP-Za-hj-np-z2-9]{4}(?:-[A-HJ-NP-Za-hj-np-z2-9]{4}){3})$", "writeOnly": true}))}
    }));
    s.insert("SessionUser".into(), object(
        &["id", "displayName", "email", "role", "permissions", "environment", "totpEnabled"],
        json!({"id": uuid(), "displayName": {"type": "string"}, "email": {"type": "string", "format": "email"}, "role": {"type": "string"}, "permissions": array(json!({"type": "string"})), "environment": {"type": "string"}, "totpEnabled": {"type": "boolean"}})
    ));
    s.insert(
        "TotpCodeRequest".into(),
        object(
            &["code"],
            json!({"code": {"type": "string", "pattern": "^[0-9]{6}$", "writeOnly": true}}),
        ),
    );
    s.insert(
        "TotpEnrollment".into(),
        object(
            &[
                "secret",
                "otpAuthUri",
                "algorithm",
                "digits",
                "periodSeconds",
            ],
            json!({
                "secret": {"type": "string", "pattern": "^[A-Z2-7]{32}$", "readOnly": true},
                "otpAuthUri": {"type": "string", "format": "uri", "readOnly": true},
                "algorithm": {"type": "string", "const": "SHA1"},
                "digits": {"type": "integer", "const": 6},
                "periodSeconds": {"type": "integer", "const": 30}
            }),
        ),
    );
    s.insert("RecoveryCodeSet".into(), object(
        &["recoveryCodes", "generatedAt"],
        json!({
            "recoveryCodes": {"type": "array", "minItems": 10, "maxItems": 10, "readOnly": true, "items": {"type": "string", "pattern": "^[A-HJ-NP-Z2-9]{4}(?:-[A-HJ-NP-Z2-9]{4}){3}$"}},
            "generatedAt": timestamp()
        })
    ));
    s.insert("AdminSession".into(), object(
        &["id", "current", "createdAt", "lastSeenAt", "expiresAt"],
        json!({"id": uuid(), "current": {"type": "boolean"}, "createdAt": timestamp(), "lastSeenAt": timestamp(), "expiresAt": timestamp()})
    ));
    s.insert(
        "SyncRunStatus".into(),
        string_enum(&[
            "queued",
            "fetching",
            "validating",
            "awaitingResolution",
            "readyToPublish",
            "completed",
            "failed",
        ]),
    );
    s.insert("StartSyncRequest".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["mappingVersion"],
        "properties": {"dryRun": {"type": "boolean", "default": false}, "mappingVersion": {"type": "string", "minLength": 1}, "cursor": nullable(json!({"type": "string"}))}
    }));
    s.insert("SyncRun".into(), object(
        &["id", "source", "dryRun", "mappingVersion", "status", "resumeCursor", "recordsSeen", "recordsValid", "conflictCount", "startedAt", "completedAt"],
        json!({
            "id": uuid(), "source": {"type": "string"}, "dryRun": {"type": "boolean"}, "mappingVersion": {"type": "string"}, "status": r("SyncRunStatus"),
            "resumeCursor": nullable(json!({"type": "string"})), "recordsSeen": counter(), "recordsValid": counter(), "conflictCount": counter(),
            "startedAt": timestamp(), "completedAt": nullable(timestamp()), "error": nullable(json!({"type": "string"}))
        })
    ));
    s.insert("SyncRunPage".into(), page("SyncRun"));
    s.insert("FieldDiff".into(), object(
        &["fieldPath", "baseValue", "localValue", "incomingValue", "sourceOwned"],
        json!({"fieldPath": {"type": "string"}, "baseValue": nullable(json!({})), "localValue": nullable(json!({})), "incomingValue": nullable(json!({})), "sourceOwned": {"type": "boolean"}})
    ));
    s.insert("SyncConflict".into(), object(
        &["id", "syncRunId", "productId", "sourceRecordId", "diffs", "resolvedAt", "resolution"],
        json!({"id": uuid(), "syncRunId": uuid(), "productId": nullable(uuid()), "sourceRecordId": {"type": "string"}, "diffs": array(r("FieldDiff")), "resolvedAt": nullable(timestamp()), "resolution": nullable(json!({"type": "string"}))})
    ));
    s.insert("SyncConflictPage".into(), page("SyncConflict"));
    s.insert("CreateTemporaryOverride".into(), object(
        &["productId", "fieldPath", "value", "reason"],
        json!({"productId": uuid(), "fieldPath": {"type": "string", "minLength": 1}, "value": {}, "reason": {"type": "string", "minLength": 10}, "expiresAt": nullable(timestamp())})
    ));
    s.insert("TemporaryOverride".into(), object(
        &["id", "productId", "fieldPath", "value", "reason", "createdAt", "expiresAt", "expired"],
        json!({"id": uuid(), "productId": uuid(), "fieldPath": {"type": "string"}, "value": {}, "reason": {"type": "string"}, "createdAt": timestamp(), "expiresAt": timestamp(), "expired": {"type": "boolean"}})
    ));
    s.insert("TemporaryOverridePage".into(), page("TemporaryOverride"));
    s.insert(
        "OperationKind".into(),
        string_enum(&[
            "migrationPreflight",
            "migrationApply",
            "backup",
            "restoreValidate",
            "retentionApply",
            "searchReindex",
            "cacheInvalidate",
            "feishuSync",
        ]),
    );
    s.insert(
        "OperationStatus".into(),
        string_enum(&["queued", "running", "completed", "failed"]),
    );
    s.insert("CreateOperationRequest".into(), object(
        &["kind", "reason", "confirmation"],
        json!({"kind": r("OperationKind"), "reason": {"type": "string", "minLength": 10}, "confirmation": {"type": "string"}})
    ));
    s.insert("BackgroundOperation".into(), object(
        &["id", "kind", "status", "reason", "createdAt", "updatedAt", "result"],
        json!({"id": uuid(), "kind": r("OperationKind"), "status": r("OperationStatus"), "reason": {"type": "string"}, "createdAt": timestamp(), "updatedAt": timestamp(), "result": nullable(json!({}))})
    ));
    s.insert(
        "BackgroundOperationPage".into(),
        page("BackgroundOperation"),
    );
    s.insert(
        "PlatformSettings".into(),
        object(
            &[
                "rfqRetentionDays",
                "retentionDeletionGraceDays",
                "temporaryOverrideDefaultDays",
                "publicLocale",
                "revision",
            ],
            json!({
                "rfqRetentionDays": {"type": "integer", "minimum": 30, "maximum": 3650},
                "retentionDeletionGraceDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "temporaryOverrideDefaultDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "publicLocale": {"type": "string", "const": "en", "readOnly": true},
                "revision": {"type": "integer", "minimum": 1, "readOnly": true}
            }),
        ),
    );
    s.insert(
        "UpdatePlatformSettings".into(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["reason"],
            "minProperties": 2,
            "properties": {
                "rfqRetentionDays": {"type": "integer", "minimum": 30, "maximum": 3650},
                "retentionDeletionGraceDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "temporaryOverrideDefaultDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "reason": {"type": "string", "minLength": 12}
            }
        }),
    );
    s.insert("AuditEvent".into(), object(
        &["id", "actor", "action", "entityType", "entityId", "before", "after", "reason", "requestId", "occurredAt"],
        json!({"id": uuid(), "actor": {"type": "string"}, "action": {"type": "string"}, "entityType": {"type": "string"}, "entityId": nullable(uuid()), "before": nullable(json!({})), "after": nullable(json!({})), "reason": nullable(json!({"type": "string"})), "requestId": uuid(), "occurredAt": timestamp()})
    ));
    s.insert("AuditEventPage".into(), page("AuditEvent"));
    s.insert("AnalyticsSummary".into(), object(
        &["acceptedEventCount", "rfqCount", "contactCount", "containsPii", "source"],
        json!({"acceptedEventCount": counter(), "rfqCount": counter(), "contactCount": counter(), "containsPii": {"type": "boolean", "const": false}, "source": {"type": "string", "const": "firstParty"}})
    ));
    #[cfg(feature = "devtools")]
    s.insert("TerminalToken".into(), object(&["token", "expiresInSeconds"], json!({"token": {"type": "string"}, "expiresInSeconds": {"type": "integer", "minimum": 1}})));
}

fn add(paths: &mut Map<String, Value>, path: &str, method: &str, operation: Value) {
    let path_item = paths
        .entry(path.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    path_item
        .as_object_mut()
        .expect("path item is an object")
        .insert(method.to_owned(), operation);
}

fn add_admin_list(
    paths: &mut Map<String, Value>,
    path: &str,
    operation_id: &str,
    summary: &str,
    page_schema: &str,
) {
    add(
        paths,
        path,
        "get",
        admin(
            params(
                op(
                    operation_id,
                    summary,
                    "admin",
                    [("200", json_response("Collection", r(page_schema)))],
                ),
                admin_pagination_params(),
            ),
            false,
        ),
    );
}

fn admin_pagination_params() -> Vec<Value> {
    vec![
        json!({
            "name": "cursor", "in": "query", "required": false,
            "description": "Opaque endpoint-scoped cursor returned by the previous page.",
            "schema": {"type": "string", "minLength": 1, "maxLength": 2048}
        }),
        json!({
            "name": "limit", "in": "query", "required": false,
            "description": "Page size; values outside 1 through 100 return Problem Details 400.",
            "schema": {"type": "integer", "minimum": 1, "maximum": 100, "default": 50}
        }),
    ]
}

#[cfg(feature = "devtools")]
fn add_devtools_paths(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/devtools/v1/sessions/token",
        "post",
        admin(
            op(
                "createDevtoolsTerminalToken",
                "Create a one-time PTY token",
                "devtools",
                [(
                    "200",
                    json_response("One-time PTY token", r("TerminalToken")),
                )],
            ),
            true,
        ),
    );
    add(
        paths,
        "/api/devtools/v1/terminal",
        "get",
        params(
            op(
                "openDevtoolsTerminal",
                "Upgrade a one-time token to a development-only PTY WebSocket",
                "devtools",
                [("101", empty_response("WebSocket protocol switch"))],
            ),
            vec![query_param(
                "token",
                true,
                json!({"type": "string", "minLength": 1}),
            )],
        ),
    );
}

fn op<const N: usize>(
    operation_id: &str,
    summary: &str,
    tag: &str,
    successes: [(&str, Value); N],
) -> Value {
    let mut responses: Map<String, Value> = successes
        .into_iter()
        .map(|(status, response)| (status.to_owned(), response))
        .collect();
    for (status, description) in [
        ("400", "Malformed request"),
        ("401", "Admin session required"),
        ("403", "Permission, CSRF, origin, or TOTP check failed"),
        ("404", "Resource or route not found"),
        ("409", "Concurrent or domain conflict"),
        ("422", "Validation failed"),
        ("428", "If-Match precondition required"),
        ("429", "Authentication rate limit exceeded"),
        ("500", "Internal server error"),
        ("503", "Required service is unavailable"),
    ] {
        responses
            .entry(status)
            .or_insert_with(|| problem_response(description));
    }
    json!({"operationId": operation_id, "summary": summary, "tags": [tag], "responses": responses})
}

fn body(mut operation: Value, schema: Value) -> Value {
    operation["requestBody"] =
        json!({"required": true, "content": {"application/json": {"schema": schema}}});
    operation
}

fn params(mut operation: Value, parameters: Vec<Value>) -> Value {
    operation["parameters"] = Value::Array(parameters);
    operation
}

fn admin(mut operation: Value, mutation: bool) -> Value {
    let mut requirement = Map::new();
    requirement.insert("adminSession".into(), json!([]));
    if mutation {
        requirement.insert("csrfToken".into(), json!([]));
    }
    operation["security"] = json!([Value::Object(requirement)]);
    operation
}

fn entity_op(operation_id: &str, summary: &str, description: &str) -> Value {
    op(
        operation_id,
        summary,
        "adminContent",
        [(
            "200",
            response_header(
                json_response(description, r("ContentEntry")),
                "ETag",
                "Current revision tag",
            ),
        )],
    )
}

fn product_entity_op(operation_id: &str, summary: &str) -> Value {
    op(
        operation_id,
        summary,
        "adminCatalog",
        [(
            "200",
            response_header(
                json_response("Product published", r("Product")),
                "ETag",
                "Published revision tag",
            ),
        )],
    )
}

fn entity_params() -> Vec<Value> {
    vec![path_param("id", uuid()), if_match_param()]
}

fn idempotent_entity_params() -> Vec<Value> {
    vec![
        path_param("id", uuid()),
        if_match_param(),
        idempotency_param(),
    ]
}

fn json_response(description: &str, schema: Value) -> Value {
    json!({"description": description, "content": {"application/json": {"schema": schema}}})
}

fn text_response(description: &str, content_type: &str, schema: Value) -> Value {
    json!({"description": description, "content": {content_type: {"schema": schema}}})
}

fn empty_response(description: &str) -> Value {
    json!({"description": description})
}

fn problem_response(description: &str) -> Value {
    json!({"description": description, "content": {"application/problem+json": {"schema": r("ProblemDetails")}}})
}

fn session_response(description: &str) -> Value {
    let mut response = json_response(description, r("SessionUser"));
    response["headers"] = json!({
        "X-CSRF-Token": {"description": "Fresh CSRF token mirrored in the readable host-only CSRF cookie.", "schema": {"type": "string"}},
        "Set-Cookie": {"description": "Host-only session and CSRF cookies.", "schema": {"type": "string"}}
    });
    response
}

fn response_header(mut response: Value, name: &str, description: &str) -> Value {
    response["headers"][name] = json!({"description": description, "schema": {"type": "string"}});
    response
}

fn path_param(name: &str, schema: Value) -> Value {
    json!({"name": name, "in": "path", "required": true, "schema": schema})
}

fn query_param(name: &str, required: bool, schema: Value) -> Value {
    json!({"name": name, "in": "query", "required": required, "schema": schema})
}

fn idempotency_param() -> Value {
    json!({"name": "Idempotency-Key", "in": "header", "required": true, "description": "Replay key scoped to this mutation. Reusing it with the same request returns the original status and entity; a different request returns 409.", "schema": {"type": "string", "minLength": 8, "maxLength": 200}})
}

fn if_match_param() -> Value {
    json!({"name": "If-Match", "in": "header", "required": true, "description": "Current entity ETag, formatted as revision-N.", "schema": {"type": "string", "pattern": "^\\\"revision-[0-9]+\\\"$"}})
}

fn optional_if_match_param() -> Value {
    json!({
        "name": "If-Match", "in": "header", "required": false,
        "description": "Optional current working revision precondition. When supplied and stale, preview issuance returns 409.",
        "schema": {"type": "string", "pattern": "^\\\"revision-[0-9]+\\\"$"}
    })
}

fn totp_param() -> Value {
    json!({
        "name": "X-TOTP-Code", "in": "header", "required": false,
        "description": "A fresh six-digit TOTP is required for migration apply, backup, restore validation, and retention operations.",
        "schema": {"type": "string", "pattern": "^[0-9]{6}$"}
    })
}

fn seo_properties() -> Value {
    json!({
        "title": nullable(json!({"type": "string"})), "description": nullable(json!({"type": "string"})),
        "canonicalPath": nullable(json!({"type": "string"})), "indexable": {"type": "boolean", "default": false}
    })
}

fn object(required: &[&str], properties: Value) -> Value {
    json!({"type": "object", "additionalProperties": false, "required": required, "properties": properties})
}

fn page(item: &str) -> Value {
    object(
        &["items", "nextCursor"],
        json!({"items": array(r(item)), "nextCursor": nullable(json!({"type": "string"}))}),
    )
}

fn product_page() -> Value {
    object(
        &["items", "nextCursor"],
        json!({
            "items": array(r("Product")),
            "nextCursor": {
                "description": "Opaque base64url v1 keyset cursor bound to the filters used for this page.",
                "anyOf": [
                    {"type": "string", "minLength": 1, "maxLength": 2048, "pattern": "^[A-Za-z0-9_-]+$"},
                    {"type": "null"}
                ]
            }
        }),
    )
}

fn array(items: Value) -> Value {
    json!({"type": "array", "items": items})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn r(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn nullable(schema: Value) -> Value {
    json!({"anyOf": [schema, {"type": "null"}]})
}

fn uuid() -> Value {
    json!({"type": "string", "format": "uuid"})
}

fn timestamp() -> Value {
    json!({"type": "string", "format": "date-time"})
}

fn revision() -> Value {
    json!({"type": "integer", "format": "int64", "minimum": 1})
}

fn counter() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn slug() -> Value {
    json!({"type": "string", "minLength": 1, "maxLength": 180, "pattern": "^[a-z0-9]+(?:-[a-z0-9]+)*$"})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_every_production_route() {
        if cfg!(feature = "devtools") {
            return;
        }
        let document = document();
        let paths = document["paths"].as_object().expect("paths object");
        let expected = [
            "/api/admin/v1/analytics/summary",
            "/api/admin/v1/audit",
            "/api/admin/v1/auth/login",
            "/api/admin/v1/auth/logout",
            "/api/admin/v1/auth/recovery-codes/regenerate",
            "/api/admin/v1/auth/session",
            "/api/admin/v1/auth/sessions",
            "/api/admin/v1/auth/sessions/{id}",
            "/api/admin/v1/auth/setup",
            "/api/admin/v1/auth/totp/confirm",
            "/api/admin/v1/auth/totp/enrollment",
            "/api/admin/v1/contacts",
            "/api/admin/v1/content",
            "/api/admin/v1/content/{id}",
            "/api/admin/v1/content/{id}/preview",
            "/api/admin/v1/content/{id}/publish",
            "/api/admin/v1/content/{id}/rollback",
            "/api/admin/v1/feishu/conflicts",
            "/api/admin/v1/feishu/sync-runs",
            "/api/admin/v1/operations",
            "/api/admin/v1/operations/{id}",
            "/api/admin/v1/operations/{id}/events",
            "/api/admin/v1/products",
            "/api/admin/v1/products/{id}/publish",
            "/api/admin/v1/products/{id}/temporary-overrides",
            "/api/admin/v1/rfqs",
            "/api/admin/v1/settings",
            "/api/public/v1/analytics/consents",
            "/api/public/v1/analytics/events",
            "/api/public/v1/contact",
            "/api/public/v1/content-preview",
            "/api/public/v1/content/{kind}/{slug}",
            "/api/public/v1/discovery",
            "/api/public/v1/products",
            "/api/public/v1/products/{slug}",
            "/api/public/v1/rfqs",
            "/api/public/v1/selector",
            "/healthz",
            "/openapi.json",
            "/readyz",
            "/robots.txt",
        ];
        let actual: Vec<_> = paths.keys().map(String::as_str).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn production_contract_contains_no_devtools_surface() {
        if !cfg!(feature = "devtools") {
            let serialized = serde_json::to_string(&document()).expect("serialize document");
            assert!(!serialized.to_ascii_lowercase().contains("devtools"));
            assert!(!serialized.to_ascii_lowercase().contains("terminaltoken"));
        }
    }

    #[test]
    fn public_intake_contract_exposes_typed_rfqs_and_receipted_analytics() {
        let document = document();
        let schemas = &document["components"]["schemas"];
        assert_eq!(
            schemas["CreateRfqRequest"]["discriminator"]["propertyName"],
            "journey"
        );
        assert_eq!(
            schemas["CreateRfqRequest"]["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            schemas["AnalyticsPolicyVersion"]["enum"],
            json!(["analytics-v1"])
        );
        let consented_required = schemas["ConsentedAnalyticsEvent"]["required"]
            .as_array()
            .expect("consented analytics required fields");
        assert!(consented_required.contains(&json!("consentReceipt")));
        assert!(consented_required.contains(&json!("policyVersion")));
        for property_shape in schemas["AnalyticsEventProperties"]["anyOf"]
            .as_array()
            .expect("analytics property allowlists")
        {
            assert_eq!(property_shape["additionalProperties"], false);
        }
    }

    #[test]
    fn admin_submit_publish_and_job_mutations_require_a_bounded_idempotency_key() {
        let document = document();
        for (path, method) in [
            ("/api/admin/v1/content", "post"),
            ("/api/admin/v1/content/{id}/publish", "post"),
            ("/api/admin/v1/content/{id}/rollback", "post"),
            ("/api/admin/v1/products/{id}/publish", "post"),
            ("/api/admin/v1/products/{id}/temporary-overrides", "post"),
            ("/api/admin/v1/feishu/sync-runs", "post"),
            ("/api/admin/v1/operations", "post"),
        ] {
            let parameters = document["paths"][path][method]["parameters"]
                .as_array()
                .unwrap_or_else(|| panic!("{method} {path} must document parameters"));
            let key = parameters
                .iter()
                .find(|parameter| parameter["name"] == "Idempotency-Key")
                .unwrap_or_else(|| panic!("{method} {path} has no Idempotency-Key"));
            assert_eq!(key["in"], "header");
            assert_eq!(key["required"], true);
            assert_eq!(key["schema"]["minLength"], 8);
            assert_eq!(key["schema"]["maxLength"], 200);
        }
    }

    #[test]
    fn every_operation_has_an_id_and_problem_contracts() {
        let document = document();
        for (path, item) in document["paths"].as_object().expect("paths object") {
            for (method, operation) in item.as_object().expect("path item") {
                assert!(
                    operation["operationId"].as_str().is_some(),
                    "{method} {path} has no operationId"
                );
                let responses = operation["responses"].as_object().expect("responses");
                assert!(
                    responses
                        .keys()
                        .any(|status| status.starts_with('2') || status == "101"),
                    "{method} {path} has no success response"
                );
                assert_eq!(
                    responses["422"]["content"]["application/problem+json"]["schema"]["$ref"],
                    "#/components/schemas/ProblemDetails"
                );
            }
        }
    }
}
