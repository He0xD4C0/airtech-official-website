//! System and public API path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;

/// Adds system endpoints and the core public API.
pub(super) fn add_core(paths: &mut Map<String, Value>) {
    add(
        paths,
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
        paths,
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
        paths,
        "/internal/metrics",
        "get",
        op(
            "getInternalMetrics",
            "Return internal low-cardinality OpenMetrics telemetry",
            "system",
            [(
                "200",
                text_response(
                    "OpenMetrics telemetry",
                    "application/openmetrics-text",
                    json!({"type": "string"}),
                ),
            )],
        ),
    );
    add(
        paths,
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
        paths,
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
        paths,
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
                        json_response("Published content", r("PublicContentProjection")),
                        "ETag",
                        "Immutable published revision tag",
                    ),
                )],
            ),
            vec![
                path_param(
                    "kind",
                    string_enum(&[
                        "home",
                        "solutions",
                        "technology",
                        "articles",
                        "news",
                        "faqs",
                        "case-studies",
                        "downloads",
                        "company",
                        "legal",
                    ]),
                ),
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
        "Get one exact content revision with a short-lived token bound to a currently authorized Admin session",
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
        paths,
        "/api/public/v1/content-preview",
        "get",
        preview_operation,
    );
    add(
        paths,
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
        paths,
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
            vec![
                path_param("slug", slug()),
                json!({
                    "name": "family",
                    "in": "query",
                    "required": false,
                    "description": "Product family used to disambiguate presentation slugs that are reusable across families.",
                    "schema": r("ProductFamily")
                }),
            ],
        ),
    );
    add(
        paths,
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
        paths,
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
        paths,
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
        paths,
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
        paths,
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
        paths,
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
}

/// Adds database-backed public-site endpoints.
pub(super) fn add_data(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/public/v1/site-bootstrap",
        "get",
        params(
            op(
                "getSiteBootstrap",
                "Get the published site shell and default company information",
                "publicSite",
                [(
                    "200",
                    json_response("Published site bootstrap", r("SiteBootstrap")),
                )],
            ),
            vec![locale_param(false)],
        ),
    );
    add(
        paths,
        "/api/public/v1/routes/resolve",
        "get",
        params(
            op(
                "resolvePublishedRoute",
                "Resolve one canonical published public route",
                "publicSite",
                [(
                    "200",
                    json_response("Resolved public route", r("RouteResolution")),
                )],
            ),
            vec![
                query_param(
                    "path",
                    true,
                    json!({
                        "type": "string",
                        "minLength": 3,
                        "maxLength": 2048,
                        "pattern": "^/en(?:/|$)[^?#]*$"
                    }),
                ),
                locale_param(false),
            ],
        ),
    );
    add(
        paths,
        "/api/public/v1/news",
        "get",
        params(
            op(
                "listPublishedNews",
                "List published News records",
                "publicNews",
                [(
                    "200",
                    json_response("Published News records", r("NewsPage")),
                )],
            ),
            vec![
                locale_param(false),
                query_param(
                    "category",
                    false,
                    json!({"type": "string", "minLength": 1, "maxLength": 120}),
                ),
                json!({
                    "name": "cursor", "in": "query", "required": false,
                    "description": "UUID cursor of the last News record returned by the previous page.",
                    "schema": {"type": "string", "format": "uuid"}
                }),
                json!({
                    "name": "limit", "in": "query", "required": false,
                    "schema": {"type": "integer", "minimum": 1, "maximum": 100, "default": 20}
                }),
            ],
        ),
    );
    add(
        paths,
        "/api/public/v1/news/{slug}",
        "get",
        params(
            op(
                "getPublishedNews",
                "Get one published News record",
                "publicNews",
                [(
                    "200",
                    response_header(
                        json_response("Published News record", r("NewsEntry")),
                        "ETag",
                        "Immutable published revision tag",
                    ),
                )],
            ),
            vec![path_param("slug", slug()), locale_param(false)],
        ),
    );
    add(
        paths,
        "/api/public/v1/guest-visits",
        "post",
        body(
            op(
                "createGuestVisit",
                "Create or refresh a consented anonymous first-party visit",
                "publicAnalytics",
                [
                    (
                        "200",
                        response_header(
                            json_response("Existing visit refreshed", r("GuestVisit")),
                            "Cache-Control",
                            "private, no-store, max-age=0",
                        ),
                    ),
                    (
                        "201",
                        response_header(
                            json_response("Anonymous visit created", r("GuestVisit")),
                            "Cache-Control",
                            "private, no-store, max-age=0",
                        ),
                    ),
                ],
            ),
            r("CreateGuestVisit"),
        ),
    );
}
