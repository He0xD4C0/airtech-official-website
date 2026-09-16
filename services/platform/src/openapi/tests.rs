use serde_json::json;

use super::document;

#[test]
fn documents_every_production_route() {
    if cfg!(feature = "devtools") {
        return;
    }
    let document = document();
    let paths = document["paths"].as_object().expect("paths object");
    let expected = [
        "/api/admin/v1/analytics/overview",
        "/api/admin/v1/analytics/sources",
        "/api/admin/v1/audit",
        "/api/admin/v1/audit/export.csv",
        "/api/admin/v1/auth/invitations/accept",
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
        "/api/admin/v1/contacts/{id}",
        "/api/admin/v1/contacts/{id}/assignment",
        "/api/admin/v1/contacts/{id}/notes",
        "/api/admin/v1/contacts/{id}/pii",
        "/api/admin/v1/contacts/{id}/status",
        "/api/admin/v1/content-drafts",
        "/api/admin/v1/content-drafts/templates",
        "/api/admin/v1/content-drafts/{draftId}",
        "/api/admin/v1/content-drafts/{draftId}/claim",
        "/api/admin/v1/content-drafts/{draftId}/shares",
        "/api/admin/v1/content-drafts/{draftId}/submit",
        "/api/admin/v1/content-drafts/{draftId}/withdraw",
        "/api/admin/v1/content-reviews",
        "/api/admin/v1/content-reviews/{draftId}/approve",
        "/api/admin/v1/content-reviews/{draftId}/reject",
        "/api/admin/v1/dashboard/summary",
        "/api/admin/v1/feishu/conflicts",
        "/api/admin/v1/feishu/conflicts/{id}/resolve",
        "/api/admin/v1/feishu/connection-status",
        "/api/admin/v1/feishu/mappings",
        "/api/admin/v1/feishu/staging",
        "/api/admin/v1/feishu/sync-runs",
        "/api/admin/v1/media/assets",
        "/api/admin/v1/media/assets/{id}",
        "/api/admin/v1/media/assets/{id}/references",
        "/api/admin/v1/operations/{id}",
        "/api/admin/v1/operations/{id}/events",
        "/api/admin/v1/products",
        "/api/admin/v1/products/imports",
        "/api/admin/v1/products/imports/{id}",
        "/api/admin/v1/products/{id}",
        "/api/admin/v1/products/{id}/presentation",
        "/api/admin/v1/products/{id}/private-pricing",
        "/api/admin/v1/products/{id}/publication-readiness",
        "/api/admin/v1/products/{id}/publish",
        "/api/admin/v1/products/{id}/temporary-overrides",
        "/api/admin/v1/products/{id}/validation-report",
        "/api/admin/v1/published-content",
        "/api/admin/v1/published-content/{contentId}",
        "/api/admin/v1/published-content/{contentId}/drafts",
        "/api/admin/v1/rfqs",
        "/api/admin/v1/rfqs/{id}",
        "/api/admin/v1/rfqs/{id}/assignment",
        "/api/admin/v1/rfqs/{id}/notes",
        "/api/admin/v1/rfqs/{id}/pii",
        "/api/admin/v1/rfqs/{id}/status",
        "/api/admin/v1/roles",
        "/api/admin/v1/roles/{id}",
        "/api/admin/v1/settings",
        "/api/admin/v1/user-invitations",
        "/api/admin/v1/user-invitations/{id}/revoke",
        "/api/admin/v1/users",
        "/api/admin/v1/users/{id}",
        "/api/admin/v1/users/{id}/sessions",
        "/api/public/v1/analytics/consents",
        "/api/public/v1/analytics/events",
        "/api/public/v1/contact",
        "/api/public/v1/content/{kind}/{slug}",
        "/api/public/v1/discovery",
        "/api/public/v1/guest-visits",
        "/api/public/v1/media/{assetId}",
        "/api/public/v1/media/{assetId}/download",
        "/api/public/v1/news",
        "/api/public/v1/news/{slug}",
        "/api/public/v1/products",
        "/api/public/v1/products/{slug}",
        "/api/public/v1/rfqs",
        "/api/public/v1/routes/resolve",
        "/api/public/v1/selector",
        "/api/public/v1/site-bootstrap",
        "/healthz",
        "/internal/metrics",
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
fn invitation_acceptance_is_public_auth_surface_with_write_only_secrets() {
    let document = document();
    let operation = &document["paths"]["/api/admin/v1/auth/invitations/accept"]["post"];
    assert_eq!(operation["operationId"], "acceptAdministratorInvitation");
    assert!(operation.get("security").is_none());
    let schema = &document["components"]["schemas"]["AcceptInvitationRequest"];
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["token"]["writeOnly"], true);
    assert_eq!(schema["properties"]["password"]["writeOnly"], true);
    assert_eq!(
        document["components"]["schemas"]["InvitationAcceptance"]["properties"]["status"]["const"],
        "active"
    );
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
fn database_driven_site_and_selector_facets_are_typed_without_fixed_values() {
    let document = document();
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["SiteBootstrap"]["properties"]["motorTechnologies"]["type"],
        "array"
    );
    assert_eq!(
        schemas["SiteBootstrap"]["properties"]["motorTechnologies"]["items"]["type"],
        "string"
    );
    assert!(schemas["SiteBootstrap"]["properties"]["motorTechnologies"]
        .get("enum")
        .is_none());
    assert!(schemas["SelectorRequest"]["properties"]
        .get("motorTechnology")
        .is_some());
    assert_eq!(
        schemas["ProductImportResult"]["properties"]["errors"]["items"]["$ref"],
        "#/components/schemas/ProductImportRowError"
    );
    assert_eq!(
        schemas["ProductImportRowError"],
        schemas["ProductImportError"]
    );
}

#[test]
fn public_editorial_contract_is_cms_v2_only() {
    let document = document();
    let schemas = &document["components"]["schemas"];
    assert_eq!(
        schemas["NewsEntry"]["properties"]["content"]["$ref"],
        "#/components/schemas/PublicContentProjection"
    );
    for property in ["generalInformation", "navigation", "footer"] {
        assert_eq!(
            schemas["SiteBootstrap"]["properties"][property]["anyOf"][0]["$ref"],
            "#/components/schemas/PublicContentProjection"
        );
        assert_eq!(
            schemas["SiteBootstrap"]["properties"][property]["anyOf"][1]["type"],
            "null"
        );
    }
    assert_eq!(
        schemas["RouteResolution"]["properties"]["page"]["anyOf"][0]["$ref"],
        "#/components/schemas/PublicContentProjection"
    );
    assert_eq!(
        document["paths"]["/api/public/v1/content/{kind}/{slug}"]["get"]["responses"]["200"]
            ["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/PublicContentProjection"
    );
    assert!(schemas.get("ContentPreviewResponse").is_none());

    let template_required = schemas["ContentTemplateDefinition"]["required"]
        .as_array()
        .expect("template required fields");
    assert!(template_required.contains(&json!("routePattern")));
}

#[test]
fn media_contract_is_synchronous_direct_and_public() {
    let document = document();
    let paths = &document["paths"];
    let upload = &paths["/api/admin/v1/media/assets"]["post"];
    assert_eq!(upload["operationId"], "uploadAdminMediaAsset");
    assert!(upload["responses"].get("201").is_some());
    assert!(upload["responses"].get("202").is_none());
    assert!(upload["responses"].get("413").is_some());
    assert!(upload["responses"].get("415").is_some());
    assert!(paths.get("/api/admin/v1/media/uploads").is_none());
    assert!(paths
        .get("/api/admin/v1/media/assets/{id}/references")
        .is_some());
    assert!(paths.get("/api/public/v1/media/{assetId}").is_some());

    let schema = &document["components"]["schemas"]["MediaAsset"];
    let required = schema["required"].as_array().expect("required fields");
    for field in [
        "id",
        "publicUrl",
        "downloadUrl",
        "originalName",
        "mediaType",
        "byteSize",
        "sha256",
        "uploadedBy",
        "createdAt",
    ] {
        assert!(required.contains(&json!(field)), "missing {field}");
    }
}

#[test]
fn stable_media_and_dependency_problem_types_are_contractual() {
    let document = document();
    let schemas = &document["components"]["schemas"];
    let expected_codes = crate::error::STABLE_DOMAIN_PROBLEM_CODES
        .iter()
        .map(|code| json!(code))
        .collect::<Vec<_>>();
    let expected_types = crate::error::STABLE_DOMAIN_PROBLEM_CODES
        .iter()
        .map(|code| json!(crate::error::problem_type_uri(code)))
        .collect::<Vec<_>>();
    assert_eq!(schemas["StableProblemCode"]["enum"], json!(expected_codes));
    assert_eq!(schemas["StableProblemType"]["enum"], json!(expected_types));
    assert_eq!(
        schemas["ProblemDetails"]["properties"]["type"]["x-stable-domain-types"],
        json!(expected_types)
    );

    for (path, method, status, code) in [
        (
            "/api/admin/v1/media/assets",
            "post",
            "409",
            crate::error::MEDIA_IDEMPOTENCY_CONFLICT,
        ),
        (
            "/api/admin/v1/media/assets",
            "post",
            "415",
            crate::error::MEDIA_DECODE_FAILED,
        ),
    ] {
        let example = &document["paths"][path][method]["responses"][status]["content"]
            ["application/problem+json"]["examples"][code]["value"];
        assert_eq!(example["type"], crate::error::problem_type_uri(code));
        assert_eq!(example["status"], status.parse::<u16>().unwrap());
    }
}

#[test]
fn public_media_contract_is_asset_resolved() {
    let document = document();
    let paths = &document["paths"];
    assert!(paths.get("/api/public/v1/media/{assetId}").is_some());

    let schemas = &document["components"]["schemas"];
    let required = schemas["PublicContentProjection"]["required"]
        .as_array()
        .expect("projection required fields");
    assert!(required.contains(&json!("resolvedMedia")));
    assert_eq!(
        schemas["PublicContentProjection"]["properties"]["resolvedMedia"]["items"]["$ref"],
        "#/components/schemas/ResolvedMedia"
    );
    for property in [
        "assetId",
        "publicUrl",
        "downloadUrl",
        "mediaType",
        "byteSize",
        "originalName",
    ] {
        assert!(schemas["ResolvedMedia"]["required"]
            .as_array()
            .expect("resolved media required fields")
            .contains(&json!(property)));
    }
}

#[test]
fn mutations_require_a_bounded_idempotency_key() {
    let document = document();
    for (path, method) in [
        ("/api/admin/v1/products/{id}/publish", "post"),
        ("/api/admin/v1/products/{id}/temporary-overrides", "post"),
        ("/api/admin/v1/media/assets", "post"),
        ("/api/admin/v1/products/imports", "post"),
        ("/api/admin/v1/products/{id}/presentation", "patch"),
        ("/api/public/v1/analytics/events", "post"),
        ("/api/admin/v1/user-invitations", "post"),
        ("/api/admin/v1/user-invitations/{id}/revoke", "post"),
        ("/api/admin/v1/users/{id}", "patch"),
        ("/api/admin/v1/roles/{id}", "patch"),
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
fn unified_content_contract_removes_dedicated_editorial_mutations() {
    let document = document();
    for path in [
        "/api/admin/v1/content",
        "/api/admin/v1/content/{id}/draft",
        "/api/admin/v1/content/{id}/snapshots",
        "/api/admin/v1/content/{id}/revisions",
        "/api/admin/v1/content/{id}/diff",
        "/api/admin/v1/content/{id}/revisions/{revision}/restore",
        "/api/public/v1/content-preview",
        "/api/admin/v1/news",
        "/api/admin/v1/news/{id}",
        "/api/admin/v1/general-information",
        "/api/admin/v1/general-information/{id}",
    ] {
        assert!(document["paths"].get(path).is_none(), "legacy path {path}");
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
            let has_success = responses
                .keys()
                .any(|status| status.starts_with('2') || status == "101");
            let is_explicitly_unavailable =
                operation["operationId"] == "startFeishuSyncRun" && responses.contains_key("409");
            assert!(
                has_success || is_explicitly_unavailable,
                "{method} {path} has no success response"
            );
            assert_eq!(
                responses["422"]["content"]["application/problem+json"]["schema"]["$ref"],
                "#/components/schemas/ProblemDetails"
            );
        }
    }
}
