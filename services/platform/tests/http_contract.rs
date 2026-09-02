use airtek_platform::{
    build_router,
    models::{
        ContentEntry, ContentKind, FactState, GeneralInformation, PerformanceCurve, Product,
        ProductFamily, PublicationStatus, RichTextDocument, SeoMetadata, SourceSnapshot, SpecValue,
        StagingRecord, StagingValidationStatus, SyncConflict, SyncRun, SyncRunStatus,
        TemporaryOverride,
    },
    AppState, Config,
};
use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::net::SocketAddr;
use tower::ServiceExt;

#[derive(Clone)]
struct TestAdminSession {
    cookie: String,
    csrf: String,
}

fn session_credentials(response: &axum::response::Response) -> TestAdminSession {
    let csrf = response
        .headers()
        .get("x-csrf-token")
        .expect("CSRF response header")
        .to_str()
        .unwrap()
        .to_owned();
    let cookie = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .filter_map(|value| value.split(';').next())
        .collect::<Vec<_>>()
        .join("; ");
    assert!(cookie.contains("airtek_admin_session="));
    TestAdminSession { cookie, csrf }
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("JSON response")
}

async fn install_published_site_shell(state: &AppState) {
    let now = chrono::Utc::now();
    let information = GeneralInformation {
        id: uuid::Uuid::new_v4(),
        locale: "en".into(),
        payload: json!({
            "brandName": "AIRTEKPOWER",
            "homePath": "/en",
            "organization": {"name": "AIRTEKPOWER"}
        }),
        status: PublicationStatus::Published,
        current_revision: 1,
        published_revision: Some(1),
        is_placeholder: false,
        updated_at: now,
    };
    let shell_content = |kind, slug: &str| ContentEntry {
        id: uuid::Uuid::new_v4(),
        kind,
        slug: slug.into(),
        locale: "en".into(),
        title: "Published site shell".into(),
        summary: None,
        body: RichTextDocument {
            schema_version: 1,
            doc: json!({"type": "doc", "content": []}),
        },
        seo: SeoMetadata::default(),
        status: PublicationStatus::Published,
        is_placeholder: false,
        current_revision: 1,
        published_revision: Some(1),
        scheduled_for: None,
        updated_at: now,
    };
    let navigation = shell_content(ContentKind::Navigation, "primary-navigation");
    let footer = shell_content(ContentKind::Footer, "primary-footer");

    let mut data = state.data.write().await;
    data.published_general_information
        .insert(information.id, information);
    data.published_content.insert(navigation.id, navigation);
    data.published_content.insert(footer.id, footer);
}

async fn setup_admin_without_totp(app: &axum::Router) -> TestAdminSession {
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/setup")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "displayName": "Test Administrator",
                        "email": "admin@example.com",
                        "password": "correct-horse-123",
                        "bootstrapToken": "test-bootstrap-token-please-change"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let csrf = response
        .headers()
        .get("x-csrf-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap().to_owned())
        .collect();
    assert_eq!(cookies.len(), 2);
    assert!(cookies
        .iter()
        .all(|cookie| cookie.contains("SameSite=Strict")));
    assert!(cookies.iter().all(|cookie| !cookie.contains("Domain=")));
    assert!(cookies
        .iter()
        .find(|cookie| cookie.starts_with("airtek_admin_session="))
        .unwrap()
        .contains("HttpOnly"));
    assert!(cookies
        .iter()
        .find(|cookie| cookie.starts_with("airtek_admin_session="))
        .unwrap()
        .contains("Path=/api"));
    assert!(!cookies
        .iter()
        .find(|cookie| cookie.starts_with("airtek_admin_csrf="))
        .unwrap()
        .contains("HttpOnly"));
    let cookie = cookies
        .iter()
        .map(|value| value.split(';').next().unwrap())
        .collect::<Vec<_>>()
        .join("; ");
    TestAdminSession { cookie, csrf }
}

async fn setup_admin(app: &axum::Router) -> TestAdminSession {
    let session = setup_admin_without_totp(app).await;
    let enrollment = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/totp/enrollment")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enrollment.status(), StatusCode::OK);
    let enrollment = response_json(enrollment).await;
    let totp = totp_rs::TOTP::from_url(enrollment["otpAuthUri"].as_str().unwrap())
        .expect("valid provisioning URI");
    let confirmed = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/totp/confirm")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"code": totp.generate_current().expect("system clock")}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.status(), StatusCode::OK);
    session
}

#[tokio::test]
async fn password_only_session_is_limited_to_totp_enrollment() {
    let state = AppState::for_test();
    let app = build_router(state);
    let session = setup_admin_without_totp(&app).await;

    let current = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(current.status(), StatusCode::OK);
    let current = response_json(current).await;
    assert_eq!(current["totpEnabled"], false);
    assert_eq!(current["permissions"], json!([]));

    let protected = app
        .oneshot(
            Request::get("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invitation_acceptance_is_unauthenticated_but_admin_origin_bound_and_strictly_typed() {
    let app = build_router(AppState::for_test());
    let body = json!({
        "token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "password": "correct-horse-123"
    });
    let wrong_origin = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/invitations/accept")
                .header(header::ORIGIN, "http://localhost:3000")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);

    let unknown_field = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/invitations/accept")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                        "password": "correct-horse-123",
                        "email": "attacker-controlled@example.com"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown_field.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let no_database = app
        .oneshot(
            Request::post("/api/admin/v1/auth/invitations/accept")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_database.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn api_origin_disallows_crawling_and_has_no_sitemaps() {
    let app = build_router(AppState::for_test());
    let robots = app
        .clone()
        .oneshot(Request::get("/robots.txt").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(robots.status(), StatusCode::OK);
    assert_eq!(
        robots.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
    let body = robots.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"User-agent: *\nDisallow: /\n");

    for path in [
        "/sitemap.xml",
        "/sitemap-products.xml",
        "/sitemap-anything.xml",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            response.headers().get("x-robots-tag").unwrap(),
            "noindex, nofollow, noarchive"
        );
    }
}

#[tokio::test]
#[cfg(not(feature = "devtools"))]
async fn production_contract_does_not_expose_devtools_by_default() {
    let app = build_router(AppState::for_test());
    let response = app
        .clone()
        .oneshot(Request::get("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let document = response_json(response).await;
    let paths = document["paths"].as_object().unwrap();
    assert!(paths.keys().all(|path| !path.contains("devtools")));

    let response = app
        .oneshot(
            Request::post("/api/devtools/v1/sessions/token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[cfg(feature = "devtools")]
async fn development_contract_exposes_one_time_terminal_tokens() {
    let app = build_router(AppState::for_test());
    let response = app
        .clone()
        .oneshot(Request::get("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let document = response_json(response).await;
    assert!(document["paths"]
        .as_object()
        .unwrap()
        .contains_key("/api/devtools/v1/terminal"));

    let session = setup_admin(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/devtools/v1/sessions/token")
                .header(header::COOKIE, session.cookie)
                .header("x-csrf-token", session.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let token = response_json(response).await;
    assert_eq!(token["expiresInSeconds"], 60);
    assert_eq!(token["token"].as_str().unwrap().len(), 32);
}

#[tokio::test]
async fn selector_fails_safe_without_validated_product_data() {
    let app = build_router(AppState::for_test());
    let response = app
        .oneshot(
            Request::post("/api/public/v1/selector")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "airflow": 1000,
                        "airflowUnit": "m3/h",
                        "pressure": 300,
                        "pressureUnit": "Pa",
                        "requiredCertifications": []
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["outcome"], "noValidatedCandidates");
    assert_eq!(body["candidates"], json!([]));
}

fn published_test_product(stable_id: &str, slug: &str, family: ProductFamily) -> Product {
    Product {
        id: uuid::Uuid::new_v4(),
        stable_id: stable_id.into(),
        model: Some(format!("TEST-{stable_id}")),
        slug: slug.into(),
        locale: "en".into(),
        family,
        subtype: None,
        motor_technology: None,
        title: format!("Published {stable_id}"),
        summary: None,
        seo: Default::default(),
        sort_order: 0,
        related_content_ids: Vec::new(),
        specifications: vec![],
        performance_curves: vec![],
        source_snapshot_id: uuid::Uuid::new_v4(),
        source_revision: "validated-source-revision".into(),
        current_revision: 1,
        published_revision: Some(1),
        status: PublicationStatus::Published,
        indexable: true,
        updated_at: chrono::Utc::now(),
    }
}

async fn insert_publishable_product(state: &AppState, product: Product) {
    let now = chrono::Utc::now();
    let run_id = uuid::Uuid::new_v4();
    let snapshot = SourceSnapshot {
        id: product.source_snapshot_id,
        connector_id: uuid::Uuid::new_v4(),
        sync_run_id: run_id,
        source_record_id: product.stable_id.clone(),
        source_revision: product.source_revision.clone(),
        checksum: "test-only-source-checksum".into(),
        source_payload: serde_json::to_value(&product).unwrap(),
        received_at: now,
    };
    let staging = StagingRecord {
        id: uuid::Uuid::new_v4(),
        sync_run_id: run_id,
        source_snapshot_id: snapshot.id,
        source_record_id: snapshot.source_record_id.clone(),
        validation_status: StagingValidationStatus::Valid,
        normalized_payload: Some(serde_json::to_value(&product).unwrap()),
        validation_errors: vec![],
        created_at: now,
    };
    let run = SyncRun {
        id: run_id,
        source: "feishu".into(),
        dry_run: false,
        mapping_version: "test-mapping-v1".into(),
        status: SyncRunStatus::ReadyToPublish,
        resume_cursor: None,
        records_seen: 1,
        records_valid: 1,
        conflict_count: 0,
        started_at: now,
        completed_at: None,
        error: None,
    };
    let mut data = state.data.write().await;
    data.product_revisions
        .entry(product.id)
        .or_default()
        .insert(product.current_revision, product.clone());
    data.source_snapshots.insert(snapshot.id, snapshot);
    data.staging_records.insert(staging.id, staging);
    data.sync_runs.insert(run.id, run);
    data.products.insert(product.id, product);
}

#[tokio::test]
async fn public_products_use_stable_opaque_keyset_cursors() {
    let state = AppState::for_test();
    let products = [
        published_test_product("stable-c", "product-c", ProductFamily::Axial),
        published_test_product("stable-a", "product-a", ProductFamily::Axial),
        published_test_product("stable-b", "product-b", ProductFamily::Centrifugal),
    ];
    {
        let mut data = state.data.write().await;
        for product in products {
            data.published_products.insert(product.id, product);
        }
    }
    let app = build_router(state.clone());
    let first = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first = response_json(first).await;
    assert_eq!(first["items"][0]["stableId"], "stable-a");
    assert_eq!(first["items"][1]["stableId"], "stable-b");
    let cursor = first["nextCursor"].as_str().unwrap();
    assert!(!cursor.contains("stable-b"));

    let second = app
        .clone()
        .oneshot(
            Request::get(format!("/api/public/v1/products?limit=2&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let second = response_json(second).await;
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["stableId"], "stable-c");
    assert!(second["nextCursor"].is_null());

    let malformed = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products?cursor=not-a-valid-cursor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let wrong_filters = app
        .oneshot(
            Request::get(format!(
                "/api/public/v1/products?family=axial&cursor={cursor}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_filters.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn public_product_detail_is_bound_to_family_and_slug() {
    let state = AppState::for_test();
    let products = [
        published_test_product("stable-axial", "shared-slug", ProductFamily::Axial),
        published_test_product(
            "stable-centrifugal",
            "shared-slug",
            ProductFamily::Centrifugal,
        ),
    ];
    {
        let mut data = state.data.write().await;
        for product in products {
            data.published_products.insert(product.id, product);
        }
    }
    let app = build_router(state);

    let axial = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products/shared-slug?family=axial")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(axial.status(), StatusCode::OK);
    assert_eq!(response_json(axial).await["stableId"], "stable-axial");

    let centrifugal = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products/shared-slug?family=centrifugal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(centrifugal.status(), StatusCode::OK);
    assert_eq!(
        response_json(centrifugal).await["stableId"],
        "stable-centrifugal"
    );

    let ambiguous = app
        .oneshot(
            Request::get("/api/public/v1/products/shared-slug")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ambiguous.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn product_presentation_has_an_independent_etag_and_never_revisions_facts() {
    let state = AppState::for_test();
    let mut product = published_test_product(
        "presentation-facts-stable",
        "presentation-facts-source",
        ProductFamily::Axial,
    );
    product.status = PublicationStatus::Draft;
    product.published_revision = None;
    product.indexable = false;
    let product_id = product.id;
    insert_publishable_product(&state, product.clone()).await;
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let body = json!({
        "locale": "en",
        "slug": "portal-owned-title",
        "title": "Portal-owned title",
        "summary": "Editorial presentation only",
        "seo": {
            "title": "Portal-owned SEO title",
            "description": "Editorial SEO description",
            "canonicalPath": "/en/products/axial/portal-owned-title",
            "indexable": false
        },
        "indexable": false,
        "sortOrder": 20,
        "relatedContentIds": [],
        "reason": "Test independent presentation revision"
    })
    .to_string();
    let request = |key: &str, revision: i64| {
        Request::patch(format!("/api/admin/v1/products/{product_id}/presentation"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", key)
            .header(header::IF_MATCH, format!("\"revision-{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.clone()))
            .unwrap()
    };

    let updated = app
        .clone()
        .oneshot(request("product-presentation-update-0001", 1))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        updated.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let updated = response_json(updated).await;
    assert_eq!(updated["currentRevision"], 1);
    assert_eq!(updated["presentation"]["revision"], 2);
    assert_eq!(updated["presentation"]["publishedRevision"], Value::Null);

    let replay = app
        .clone()
        .oneshot(request("product-presentation-update-0001", 1))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(
        replay.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );

    let stale = app
        .oneshot(request("product-presentation-update-stale", 1))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);

    let data = state.data.read().await;
    assert_eq!(data.products.get(&product_id), Some(&product));
    assert_eq!(
        data.product_revisions
            .get(&product_id)
            .map(std::collections::BTreeMap::len),
        Some(1)
    );
    assert_eq!(
        data.product_presentations
            .get(&(product_id, "en".to_owned()))
            .map(|presentation| presentation.revision),
        Some(2)
    );
}

#[tokio::test]
async fn public_rfq_rejects_attachment_fields() {
    let app = build_router(AppState::for_test());
    let response = app
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "rfq-no-files-0001")
                .body(Body::from(
                    json!({
                        "journey": "selection",
                        "contact": {"name": "Buyer", "email": "buyer@example.com"},
                        "sourcePath": "/en/request-a-quote/selection",
                        "locale": "en",
                        "consent": true,
                        "context": {"attachmentName": "private.pdf"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
}

#[tokio::test]
async fn public_submissions_are_idempotent() {
    let app = build_router(AppState::for_test());
    let body = json!({
        "journey": "selection",
        "contact": {"name": "Buyer", "email": "buyer@example.com"},
        "sourcePath": "/en/request-a-quote/selection",
        "locale": "en",
        "consent": true,
        "context": {
            "application": "confidentially supplied to sales",
            "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
        }
    })
    .to_string();
    let request = || {
        Request::post("/api/public/v1/rfqs")
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotency-key", "rfq-idempotent-0001")
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let first = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = response_json(first).await;
    let replay = app.oneshot(request()).await.unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay).await, first_body);
}

#[tokio::test]
async fn admin_content_creation_is_concurrently_idempotent_and_key_bound() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let body = json!({
        "kind": "article",
        "slug": "concurrent-idempotency",
        "locale": "en",
        "title": "Concurrent idempotency",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {"indexable": false},
        "isPlaceholder": true
    })
    .to_string();
    let request = |body: String, key: &'static str| {
        Request::post("/api/admin/v1/content")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", key)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    };

    let (first, replay) = tokio::join!(
        app.clone()
            .oneshot(request(body.clone(), "admin-content-concurrent-0001")),
        app.clone()
            .oneshot(request(body.clone(), "admin-content-concurrent-0001")),
    );
    let first = first.unwrap();
    let replay = replay.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(first.headers().get(header::ETAG).unwrap(), "\"revision-1\"");
    assert_eq!(
        replay.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(response_json(first).await, response_json(replay).await);

    let different = app
        .clone()
        .oneshot(request(
            body.replace("Concurrent idempotency", "Different request"),
            "admin-content-concurrent-0001",
        ))
        .await
        .unwrap();
    assert_eq!(different.status(), StatusCode::CONFLICT);

    for key in [None, Some("short".to_owned()), Some("x".repeat(201))] {
        let mut request = Request::post("/api/admin/v1/content")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.clone()))
            .unwrap();
        if let Some(key) = key {
            request
                .headers_mut()
                .insert("idempotency-key", key.parse().unwrap());
        }
        let rejected = app.clone().oneshot(request).await.unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    }

    let data = state.data.read().await;
    assert_eq!(data.content.len(), 1);
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "content.create")
            .count(),
        1
    );
}

#[tokio::test]
async fn admin_settings_are_allowlisted_concurrent_and_audited() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/settings")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let settings = response_json(response).await;
    assert_eq!(settings["publicLocale"], "en");
    assert_eq!(settings["rfqRetentionDays"], 365);

    let request = |body: Value, etag: Option<&str>| {
        let mut request = Request::patch("/api/admin/v1/settings")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-request-id", "50000000-0000-4000-8000-000000000001")
            .body(Body::from(body.to_string()))
            .unwrap();
        if let Some(etag) = etag {
            request
                .headers_mut()
                .insert(header::IF_MATCH, etag.parse().unwrap());
        }
        request
    };

    let missing_precondition = app
        .clone()
        .oneshot(request(
            json!({
                "rfqRetentionDays": 400,
                "reason": "Increase retention for current policy"
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        missing_precondition.status(),
        StatusCode::PRECONDITION_REQUIRED
    );

    for body in [
        json!({
            "publicLocale": "fr",
            "reason": "Attempt to change a read-only value"
        }),
        json!({
            "rfqRetentionDays": 400,
            "deploymentSecret": "must-not-be-writable",
            "reason": "Attempt to write deployment policy"
        }),
    ] {
        let rejected = app
            .clone()
            .oneshot(request(body, Some("\"revision-1\"")))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    }

    let invalid = app
        .clone()
        .oneshot(request(
            json!({"rfqRetentionDays": 29, "reason": "too short"}),
            Some("\"revision-1\""),
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let updated = app
        .clone()
        .oneshot(request(
            json!({
                "rfqRetentionDays": 400,
                "retentionDeletionGraceDays": 45,
                "temporaryOverrideDefaultDays": 21,
                "reason": "Align retention with approved policy"
            }),
            Some("\"revision-1\""),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        updated.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let updated_body = response_json(updated).await;
    assert_eq!(updated_body["rfqRetentionDays"], 400);
    assert_eq!(updated_body["retentionDeletionGraceDays"], 45);
    assert_eq!(updated_body["temporaryOverrideDefaultDays"], 21);
    assert_eq!(updated_body["publicLocale"], "en");
    assert_eq!(updated_body["revision"], 2);

    let stale = app
        .clone()
        .oneshot(request(
            json!({
                "rfqRetentionDays": 401,
                "reason": "Stale concurrent policy change"
            }),
            Some("\"revision-1\""),
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        state
            .integer_setting("rfqRetentionDays", 365, 30, 3_650)
            .await
            .unwrap(),
        400
    );
    let data = state.data.read().await;
    let audit = data
        .audit_events
        .iter()
        .find(|event| event.action == "settings.update")
        .expect("settings audit event");
    assert_eq!(audit.actor, "admin@example.com");
    assert_eq!(
        audit.reason.as_deref(),
        Some("Align retention with approved policy")
    );
    assert_eq!(
        audit.request_id,
        uuid::Uuid::parse_str("50000000-0000-4000-8000-000000000001").unwrap()
    );
    assert_eq!(audit.before.as_ref().unwrap()["revision"], 1);
    assert_eq!(audit.after.as_ref().unwrap()["revision"], 2);
}

fn public_contact_request(index: usize, peer: SocketAddr, forwarded_for: &str) -> Request<Body> {
    let mut request = Request::post("/api/public/v1/contact")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", format!("contact-rate-{index:04}"))
        .header("x-forwarded-for", forwarded_for)
        .body(Body::from(
            json!({
                "contact": {"name": "Buyer", "email": "buyer@example.com"},
                "topic": "General inquiry",
                "message": "Please contact our engineering procurement team.",
                "sourcePath": "/en/company/contact",
                "locale": "en",
                "consent": true
            })
            .to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(peer));
    request
}

#[tokio::test]
async fn untrusted_peer_cannot_bypass_public_limit_with_forwarded_headers() {
    let app = build_router(AppState::for_test());
    let peer: SocketAddr = "198.51.100.20:41000".parse().unwrap();
    for index in 0..5 {
        let response = app
            .clone()
            .oneshot(public_contact_request(
                index,
                peer,
                &format!("203.0.113.{}", index + 1),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let blocked = app
        .oneshot(public_contact_request(5, peer, "203.0.113.250"))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        blocked.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
}

#[tokio::test]
async fn configured_proxy_uses_the_nearest_untrusted_forwarded_client() {
    let mut config = Config::for_test();
    config.trusted_proxy_cidrs = vec!["172.28.0.10/32".parse().unwrap()];
    let app = build_router(AppState::new(config).unwrap());
    let gateway: SocketAddr = "172.28.0.10:41000".parse().unwrap();
    for index in 0..6 {
        let response = app
            .clone()
            .oneshot(public_contact_request(
                index,
                gateway,
                &format!("203.0.113.{}", index + 1),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}

#[tokio::test]
async fn rfq_and_analytics_routes_enforce_their_public_limits() {
    let rfq_app = build_router(AppState::for_test());
    let rfq_body = json!({
        "journey": "selection",
        "contact": {"name": "Buyer", "email": "buyer@example.com"},
        "sourcePath": "/en/request-a-quote/selection",
        "locale": "en",
        "consent": true,
        "context": {
            "application": "Industrial cooling",
            "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
        }
    })
    .to_string();
    for index in 0..6 {
        let response = rfq_app
            .clone()
            .oneshot(
                Request::post("/api/public/v1/rfqs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", format!("rfq-rate-{index:04}"))
                    .body(Body::from(rfq_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let expected = if index < 5 {
            StatusCode::CREATED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(response.status(), expected);
    }

    let analytics_app = build_router(AppState::for_test());
    let analytics_body = json!({
        "eventName": "pageView",
        "sourcePath": "/en",
        "locale": "en",
        "consentGranted": false,
        "properties": {}
    })
    .to_string();
    for index in 0..=120 {
        let response = analytics_app
            .clone()
            .oneshot(
                Request::post("/api/public/v1/analytics/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(analytics_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let expected = if index < 120 {
            StatusCode::ACCEPTED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(response.status(), expected);
    }
}

#[tokio::test]
async fn analytics_requires_a_current_server_receipt_and_strict_event_dictionary() {
    let app = build_router(AppState::for_test());
    let session_id = uuid::Uuid::new_v4();
    let consent = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/consents")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "anonymousSessionId": session_id,
                        "policyVersion": "analytics-v1",
                        "analyticsAllowed": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(consent.status(), StatusCode::CREATED);
    assert_eq!(
        consent.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let consent = response_json(consent).await;
    let receipt = consent["consentReceipt"].as_str().unwrap();

    let event = |event_name: &str, properties: Value, session: uuid::Uuid, receipt: &str| {
        json!({
            "eventName": event_name,
            "anonymousSessionId": session,
            "sourcePath": "/en/products",
            "locale": "en",
            "consentGranted": true,
            "policyVersion": "analytics-v1",
            "consentReceipt": receipt,
            "properties": properties
        })
        .to_string()
    };
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(event(
                    "filterApplied",
                    json!({"filterName": "family", "resultCount": 2}),
                    session_id,
                    receipt,
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(response).await["accepted"], true);

    for (event_name, properties) in [
        ("inventedEvent", json!({})),
        ("pageView", json!({"email": "buyer@example.com"})),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": {"nested": true}}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "buyer@example.com"}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "+1 (555) 010-1234"}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "x".repeat(161)}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "Jane Doe"}),
        ),
        (
            "faqExpanded",
            json!({"faqId": "faq-1", "category": "arbitrary-free-text"}),
        ),
        ("pageView", json!({"contentKind": "Jane Doe"})),
        (
            "filterApplied",
            json!({"filterName": "jane-doe", "resultCount": 2}),
        ),
        ("faqExpanded", json!({"faqId": "jane-doe"})),
        (
            "ctaClicked",
            json!({"ctaId": "private-note", "placement": "hero"}),
        ),
        (
            "ctaClicked",
            json!({"ctaId": "request-quote", "placement": "jane-doe"}),
        ),
        (
            "ctaClicked",
            json!({"ctaId": "request-quote", "placement": "hero", "destinationPath": "/en/jane-doe"}),
        ),
        (
            "rfqValidationError",
            json!({
                "journey": "product",
                "step": 1,
                "fieldName": "jane-doe",
                "errorCode": "publishedContextRequired"
            }),
        ),
        (
            "rfqSubmitFailed",
            json!({"journey": "product", "errorCode": "jane-doe"}),
        ),
    ] {
        let rejected = app
            .clone()
            .oneshot(
                Request::post("/api/public/v1/analytics/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(event(
                        event_name, properties, session_id, receipt,
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    let missing_receipt = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "eventName": "pageView",
                        "anonymousSessionId": session_id,
                        "sourcePath": "/en",
                        "locale": "en",
                        "consentGranted": true,
                        "policyVersion": "analytics-v1",
                        "properties": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_receipt.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let mismatched_session = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(event(
                    "pageView",
                    json!({}),
                    uuid::Uuid::new_v4(),
                    receipt,
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        mismatched_session.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn analytics_denial_supersedes_an_existing_allow_receipt() {
    let app = build_router(AppState::for_test());
    let session_id = uuid::Uuid::new_v4();
    let decision = |allowed: bool| {
        Request::post("/api/public/v1/analytics/consents")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "anonymousSessionId": session_id,
                    "policyVersion": "analytics-v1",
                    "analyticsAllowed": allowed
                })
                .to_string(),
            ))
            .unwrap()
    };
    let allowed = app.clone().oneshot(decision(true)).await.unwrap();
    let allowed = response_json(allowed).await;
    let receipt = allowed["consentReceipt"].as_str().unwrap();
    let denied = app.clone().oneshot(decision(false)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::CREATED);

    let event = app
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "eventName": "pageView",
                        "anonymousSessionId": session_id,
                        "sourcePath": "/en",
                        "locale": "en",
                        "consentGranted": true,
                        "policyVersion": "analytics-v1",
                        "consentReceipt": receipt,
                        "properties": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(event.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn analytics_consent_rejects_a_client_selected_policy_version() {
    let app = build_router(AppState::for_test());
    let response = app
        .oneshot(
            Request::post("/api/public/v1/analytics/consents")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "anonymousSessionId": uuid::Uuid::new_v4(),
                        "policyVersion": "client-invented-policy",
                        "analyticsAllowed": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn product_rfq_requires_an_exact_published_product_snapshot() {
    let state = AppState::for_test();
    let id = uuid::Uuid::new_v4();
    let product = Product {
        id,
        stable_id: "validated-stable-id".into(),
        model: Some("VALIDATED-MODEL".into()),
        slug: "validated-product".into(),
        locale: "en".into(),
        family: ProductFamily::Axial,
        subtype: None,
        motor_technology: None,
        title: "Validated published product".into(),
        summary: None,
        seo: Default::default(),
        sort_order: 0,
        related_content_ids: Vec::new(),
        specifications: vec![],
        performance_curves: vec![],
        source_snapshot_id: uuid::Uuid::new_v4(),
        source_revision: "source-revision-1".into(),
        current_revision: 4,
        published_revision: Some(4),
        status: PublicationStatus::Published,
        indexable: true,
        updated_at: chrono::Utc::now(),
    };
    state
        .data
        .write()
        .await
        .published_products
        .insert(id, product);
    let app = build_router(state.clone());
    let body = |stable_id: &str| {
        json!({
            "journey": "product",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "productContext": {
                "productId": id,
                "stableId": stable_id,
                "model": "VALIDATED-MODEL",
                "publishedRevision": 4
            },
            "sourcePath": "/en/request-a-quote/product",
            "locale": "en",
            "consent": true,
            "context": {"application": "Industrial cooling", "quantity": 10}
        })
        .to_string()
    };
    let valid = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "exact-product-context")
                .body(Body::from(body("validated-stable-id")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(valid.status(), StatusCode::CREATED);
    let valid_body = response_json(valid).await;
    state.data.write().await.published_products.remove(&id);
    let replay = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "exact-product-context")
                .body(Body::from(body("validated-stable-id")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay).await, valid_body);
    let stale = app
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "stale-product-context")
                .body(Body::from(body("stale-or-forged-id")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn each_non_product_rfq_journey_accepts_only_its_typed_context() {
    let valid_contexts = [
        (
            "selection",
            json!({
                "application": "Industrial cooling",
                "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"},
                "requiredCertifications": ["Customer-specified certification"]
            }),
        ),
        (
            "project",
            json!({
                "application": "Air handling system",
                "projectStage": "Engineering",
                "projectScale": "Multiple air handling units"
            }),
        ),
        (
            "replacement",
            json!({
                "application": "Existing cooling assembly",
                "existingModel": "Nameplate text supplied by buyer",
                "dutyPoint": {"airflow": 850, "airflowUnit": "CFM", "pressure": 320, "pressureUnit": "Pa"},
                "replacementGoal": "Engineering review"
            }),
        ),
    ];
    for (index, (journey, context)) in valid_contexts.into_iter().enumerate() {
        let app = build_router(AppState::for_test());
        let response = app
            .oneshot(
                Request::post("/api/public/v1/rfqs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", format!("typed-rfq-{index:04}"))
                    .body(Body::from(
                        json!({
                            "journey": journey,
                            "contact": {"name": "Buyer", "email": "buyer@example.com"},
                            "sourcePath": format!("/en/request-a-quote/{journey}"),
                            "locale": "en",
                            "consent": true,
                            "context": context
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "{journey}");
    }
}

#[tokio::test]
async fn rfq_discriminator_rejects_missing_conditional_invalid_and_unknown_fields() {
    let invalid_requests = [
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {"application": "Industrial cooling"}
        }),
        json!({
            "journey": "project",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/project", "locale": "en", "consent": true,
            "context": {"application": "New air handling system"}
        }),
        json!({
            "journey": "replacement",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/replacement", "locale": "en", "consent": true,
            "context": {
                "application": "Existing cooling assembly",
                "dutyPoint": {"airflow": 850, "airflowUnit": "CFM", "pressure": 320, "pressureUnit": "Pa"}
            }
        }),
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {
                "application": "Industrial cooling", "quantity": 1.5,
                "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
            }
        }),
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {
                "application": "Industrial cooling", "unapprovedField": "bypass",
                "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
            }
        }),
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {
                "application": "Industrial cooling",
                "dutyPoint": {"airflow": 0, "airflowUnit": "watts", "pressure": -1, "pressureUnit": "psi"}
            }
        }),
    ];
    for (index, body) in invalid_requests.into_iter().enumerate() {
        let app = build_router(AppState::for_test());
        let response = app
            .oneshot(
                Request::post("/api/public/v1/rfqs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", format!("invalid-typed-rfq-{index:04}"))
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
    }
}

#[tokio::test]
async fn content_updates_require_the_current_etag() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let body = json!({
        "kind": "article",
        "slug": "safe-placeholder",
        "locale": "en",
        "title": "Safe placeholder",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {"indexable": false},
        "isPlaceholder": true
    })
    .to_string();
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "content-etag-create-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    assert_eq!(
        created.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    let id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let missing_precondition = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        missing_precondition.status(),
        StatusCode::PRECONDITION_REQUIRED
    );

    let updated = app
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::IF_MATCH, "\"revision-1\"")
                .header("idempotency-key", "content-etag-update-0001")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        updated.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
}

#[tokio::test]
async fn general_information_locale_is_immutable_after_creation() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let payload = json!({
        "brandName": "AIRTEKPOWER",
        "brandLine": null,
        "homePath": "/en",
        "footerStatement": null,
        "copyrightText": null,
        "defaultSeo": {"title": null, "description": null},
        "organization": {"name": "AIRTEKPOWER"}
    });
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/general-information")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "general-information-locale-create-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"locale": "en", "payload": payload, "isPlaceholder": false}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let rejected = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/general-information/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "general-information-locale-update-0001")
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"locale": "fr", "payload": payload, "isPlaceholder": false}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let problem = response_json(rejected).await;
    assert_eq!(
        problem["errors"]["locale"],
        json!(["Locale is immutable after General Information is created."])
    );

    let unchanged = app
        .oneshot(
            Request::get(format!("/api/admin/v1/general-information/{id}"))
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unchanged.status(), StatusCode::OK);
    let unchanged = response_json(unchanged).await;
    assert_eq!(unchanged["locale"], "en");
    assert_eq!(unchanged["currentRevision"], 1);
}

#[tokio::test]
async fn product_publish_and_temporary_override_replays_preserve_one_side_effect() {
    let state = AppState::for_test();
    let mut product = published_test_product(
        "idempotent-product",
        "idempotent-product",
        ProductFamily::Axial,
    );
    product.status = PublicationStatus::Draft;
    product.published_revision = None;
    let product_id = product.id;
    insert_publishable_product(&state, product).await;
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;

    let publish = |if_match: &'static str| {
        Request::post(format!("/api/admin/v1/products/{product_id}/publish"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "product-publish-replay-0001")
            .header(header::IF_MATCH, if_match)
            .body(Body::empty())
            .unwrap()
    };
    let first = app
        .clone()
        .oneshot(publish("\"revision-1\""))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(first.headers().get(header::ETAG).unwrap(), "\"revision-1\"");
    let first_body = response_json(first).await;
    let replay = app
        .clone()
        .oneshot(publish("\"revision-1\""))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(
        replay.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(response_json(replay).await, first_body);
    let different = app
        .clone()
        .oneshot(publish("\"revision-2\""))
        .await
        .unwrap();
    assert_eq!(different.status(), StatusCode::CONFLICT);

    let override_body = json!({
        "productId": product_id,
        "fieldPath": "specifications.voltage",
        "value": "230 V",
        "reason": "Temporary verified engineering correction",
        "expiresAt": chrono::Utc::now() + chrono::Duration::days(1)
    })
    .to_string();
    let create_override = |body: String| {
        Request::post(format!(
            "/api/admin/v1/products/{product_id}/temporary-overrides"
        ))
        .header(header::COOKIE, &session.cookie)
        .header("x-csrf-token", &session.csrf)
        .header("idempotency-key", "product-override-replay-0001")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
    };
    let first_override = app
        .clone()
        .oneshot(create_override(override_body.clone()))
        .await
        .unwrap();
    assert_eq!(first_override.status(), StatusCode::CREATED);
    let first_override_body = response_json(first_override).await;
    let replay_override = app
        .clone()
        .oneshot(create_override(override_body.clone()))
        .await
        .unwrap();
    assert_eq!(replay_override.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay_override).await, first_override_body);
    let different_override = app
        .clone()
        .oneshot(create_override(override_body.replace(
            "Temporary verified engineering correction",
            "Different temporary engineering correction",
        )))
        .await
        .unwrap();
    assert_eq!(different_override.status(), StatusCode::CONFLICT);

    let data = state.data.read().await;
    assert_eq!(data.published_products.len(), 1);
    assert_eq!(data.temporary_overrides.len(), 1);
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "product.publish")
            .count(),
        1
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "product.override.create")
            .count(),
        1
    );
}

#[tokio::test]
async fn product_publish_gate_rejects_invalid_master_and_unaccepted_workflow_state() {
    let state = AppState::for_test();
    let mut invalid = published_test_product(
        "publish-gate-invalid-master",
        "publish-gate-invalid-master",
        ProductFamily::Axial,
    );
    invalid.status = PublicationStatus::Draft;
    invalid.published_revision = None;
    invalid.specifications = vec![SpecValue {
        key: "inputPower".into(),
        label: "Input power".into(),
        value: Some(json!(100)),
        unit: Some("watts-ish".into()),
        operating_condition: None,
        state: FactState::PendingVerification,
        source_reference: Some("feishu:test:input-power".into()),
    }];
    invalid.performance_curves = vec![PerformanceCurve {
        airflow_unit: "litres/minute".into(),
        pressure_unit: "Pa".into(),
        speed_rpm: Some(0),
        density_kg_m3: Some(1.2),
        voltage: None,
        test_method: None,
        source_reference: "feishu:test:curve".into(),
        state: FactState::Verified,
        points: vec![airtek_platform::models::CurvePoint {
            airflow: 0.0,
            pressure: 1.0,
        }],
    }];
    let invalid_id = invalid.id;
    insert_publishable_product(&state, invalid).await;

    let mut blocked = published_test_product(
        "publish-gate-blocked-workflow",
        "publish-gate-blocked-workflow",
        ProductFamily::Centrifugal,
    );
    blocked.status = PublicationStatus::Draft;
    blocked.published_revision = None;
    let blocked_id = blocked.id;
    insert_publishable_product(&state, blocked.clone()).await;
    {
        let mut data = state.data.write().await;
        let staging = data
            .staging_records
            .values_mut()
            .find(|record| record.source_snapshot_id == blocked.source_snapshot_id)
            .unwrap();
        staging.validation_status = StagingValidationStatus::Conflicted;
        let run_id = staging.sync_run_id;
        let now = chrono::Utc::now();
        let conflict = SyncConflict {
            id: uuid::Uuid::new_v4(),
            sync_run_id: run_id,
            product_id: Some(blocked_id),
            source_record_id: blocked.stable_id.clone(),
            diffs: vec![],
            resolved_at: None,
            resolution: None,
        };
        data.conflicts.insert(conflict.id, conflict);
        let expired_override = TemporaryOverride {
            id: uuid::Uuid::new_v4(),
            product_id: blocked_id,
            field_path: "specifications.inputPower".into(),
            value: json!(100),
            reason: "Test-only expired engineering override".into(),
            created_at: now - chrono::Duration::days(2),
            expires_at: now - chrono::Duration::days(1),
            expired: true,
        };
        data.temporary_overrides
            .insert(expired_override.id, expired_override);
    }

    let app = build_router(state);
    let session = setup_admin(&app).await;
    let publish = |id: uuid::Uuid, key: &'static str| {
        Request::post(format!("/api/admin/v1/products/{id}/publish"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", key)
            .header(header::IF_MATCH, "\"revision-1\"")
            .body(Body::empty())
            .unwrap()
    };

    let invalid_response = app
        .clone()
        .oneshot(publish(invalid_id, "invalid-master-gate-0001"))
        .await
        .unwrap();
    assert_eq!(invalid_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let invalid_problem = response_json(invalid_response).await;
    assert!(invalid_problem["errors"]["specifications[0].unit"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty()));
    assert!(invalid_problem["errors"]["performanceCurves[0].points"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty()));

    let blocked_response = app
        .oneshot(publish(blocked_id, "blocked-workflow-gate-0001"))
        .await
        .unwrap();
    assert_eq!(blocked_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let blocked_problem = response_json(blocked_response).await;
    for field in [
        "staging.validationStatus",
        "conflicts",
        "temporaryOverrides",
    ] {
        assert!(
            blocked_problem["errors"][field]
                .as_array()
                .is_some_and(|errors| !errors.is_empty()),
            "{field}"
        );
    }
}

#[tokio::test]
async fn admin_business_mutations_require_session_and_csrf() {
    let app = build_router(AppState::for_test());
    let body = json!({
        "kind": "article",
        "slug": "csrf-contract",
        "locale": "en",
        "title": "CSRF contract",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {"indexable": false},
        "isPlaceholder": true
    })
    .to_string();
    let unauthenticated = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let session = setup_admin(&app).await;
    let missing_csrf = app
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, session.cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn identity_mutations_enforce_and_replay_idempotency_keys() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let user_id = *state
        .data
        .read()
        .await
        .admin_users
        .keys()
        .next()
        .expect("setup administrator");
    let update_body = json!({
        "displayName": "Idempotent Administrator",
        "reason": "Verify identity mutation idempotency"
    })
    .to_string();
    let update_request = |key: Option<&str>| {
        let mut request = Request::patch(format!("/api/admin/v1/users/{user_id}"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::IF_MATCH, "\"revision-1\"")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(key) = key {
            request = request.header("idempotency-key", key);
        }
        request.body(Body::from(update_body.clone())).unwrap()
    };

    let missing = app.clone().oneshot(update_request(None)).await.unwrap();
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    assert!(response_json(missing).await["detail"]
        .as_str()
        .unwrap()
        .contains("Idempotency-Key"));

    let key = "identity-user-update-replay-0001";
    let first = app
        .clone()
        .oneshot(update_request(Some(key)))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_etag = first.headers().get(header::ETAG).unwrap().clone();
    let first_body = first.into_body().collect().await.unwrap().to_bytes();
    let replay = app
        .clone()
        .oneshot(update_request(Some(key)))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(replay.headers().get(header::ETAG).unwrap(), first_etag);
    assert_eq!(
        replay.into_body().collect().await.unwrap().to_bytes(),
        first_body
    );

    let missing_invitation_key = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/user-invitations")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "email": "invitee@example.com",
                        "displayName": "Invitee",
                        "roleKeys": ["content-editor"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_invitation_key.status(), StatusCode::BAD_REQUEST);

    let missing_revoke_key = app
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/admin/v1/user-invitations/{}/revoke",
                uuid::Uuid::new_v4()
            ))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"reason": "Revoke unused invitation safely"}).to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_revoke_key.status(), StatusCode::BAD_REQUEST);

    let missing_role_key = app
        .oneshot(
            Request::patch(format!("/api/admin/v1/roles/{}", uuid::Uuid::new_v4()))
                .header(header::COOKIE, session.cookie)
                .header("x-csrf-token", session.csrf)
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "displayName": "Auditable Role",
                        "reason": "Verify role mutation idempotency"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_role_key.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logout_requires_csrf_for_an_active_session_before_revocation() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let rejected = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let still_active = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(still_active.status(), StatusCode::OK);
    let refreshed_csrf = still_active
        .headers()
        .get("x-csrf-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let logged_out = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", refreshed_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);

    let revoked = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    let idempotent = app
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::ORIGIN, "http://localhost:3100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idempotent.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn bootstrap_token_is_not_evaluated_after_initial_setup() {
    let app = build_router(AppState::for_test());
    let _ = setup_admin(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/admin/v1/auth/setup")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "displayName": "Second Administrator",
                        "email": "second@example.com",
                        "password": "another-password-123",
                        "bootstrapToken": "intentionally-wrong-token-value"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn public_projection_survives_draft_edits_and_rollback_republishes_history() {
    let state = AppState::for_test();
    install_published_site_shell(&state).await;
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let original = json!({
        "kind": "article",
        "slug": "immutable-projection",
        "locale": "en",
        "title": "Original published title",
        "summary": "Published discovery summary",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": [{"type": "paragraph"}]}},
        "seo": {"indexable": true, "canonicalPath": "/en/resources/articles/immutable-projection"},
        "isPlaceholder": false
    });
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "projection-create-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(original.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let publish_request = || {
        Request::post(format!("/api/admin/v1/content/{id}/publish"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "projection-publish-0001")
            .header(header::IF_MATCH, "\"revision-1\"")
            .body(Body::empty())
            .unwrap()
    };
    let published = app.clone().oneshot(publish_request()).await.unwrap();
    assert_eq!(published.status(), StatusCode::OK);
    assert_eq!(
        published.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    let published_body = response_json(published).await;
    let publish_replay = app.clone().oneshot(publish_request()).await.unwrap();
    assert_eq!(publish_replay.status(), StatusCode::OK);
    assert_eq!(
        publish_replay.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(response_json(publish_replay).await, published_body);

    let mut draft = original.clone();
    draft["title"] = json!("Unpublished replacement title");
    let updated = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::IF_MATCH, "\"revision-1\"")
                .header("idempotency-key", "projection-update-0001")
                .body(Body::from(draft.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let public_before_rollback = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content/articles/immutable-projection")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public_before_rollback.status(), StatusCode::OK);
    assert_eq!(
        public_before_rollback.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(
        response_json(public_before_rollback).await["title"],
        "Original published title"
    );

    let rollback_body =
        json!({"revision": 1, "reason": "Restore validated public copy"}).to_string();
    let rollback_request = || {
        Request::post(format!("/api/admin/v1/content/{id}/rollback"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "projection-rollback-0001")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, "\"revision-2\"")
            .body(Body::from(rollback_body.clone()))
            .unwrap()
    };
    let rolled_back = app.clone().oneshot(rollback_request()).await.unwrap();
    assert_eq!(rolled_back.status(), StatusCode::OK);
    assert_eq!(
        rolled_back.headers().get(header::ETAG).unwrap(),
        "\"revision-3\""
    );
    let rolled_back_body = response_json(rolled_back).await;
    let rollback_replay = app.clone().oneshot(rollback_request()).await.unwrap();
    assert_eq!(rollback_replay.status(), StatusCode::OK);
    assert_eq!(
        rollback_replay.headers().get(header::ETAG).unwrap(),
        "\"revision-3\""
    );
    assert_eq!(response_json(rollback_replay).await, rolled_back_body);

    let public_after_rollback = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content/articles/immutable-projection")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public_after_rollback.status(), StatusCode::OK);
    assert_eq!(
        public_after_rollback.headers().get(header::ETAG).unwrap(),
        "\"revision-3\""
    );
    assert_eq!(
        response_json(public_after_rollback).await["title"],
        "Original published title"
    );
    let discovery = app
        .oneshot(
            Request::get("/api/public/v1/discovery")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(discovery.status(), StatusCode::OK);
    let discovery = response_json(discovery).await;
    assert_eq!(discovery["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        discovery["entries"][0]["path"],
        "/en/resources/articles/immutable-projection"
    );
    assert_eq!(discovery["entries"][0]["title"], "Original published title");
    assert_eq!(
        discovery["entries"][0]["summary"],
        "Published discovery summary"
    );
    let data = state.data.read().await;
    assert_eq!(data.outbox_events.len(), 2);
    assert_eq!(
        data.content_revisions[&uuid::Uuid::parse_str(&id).unwrap()].len(),
        2
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "content.publish")
            .count(),
        1
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "content.rollback")
            .count(),
        1
    );
}

#[tokio::test]
async fn generic_content_endpoints_exclude_news_and_fail_closed_for_news_mutations() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let content = json!({
        "kind": "news",
        "slug": "dedicated-news-boundary",
        "locale": "en",
        "title": "Dedicated News boundary",
        "summary": "News metadata must remain transactionally consistent.",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {
            "indexable": true,
            "canonicalPath": "/en/resources/news/dedicated-news-boundary"
        },
        "isPlaceholder": false
    });

    let generic_create = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "generic-news-rejected-create")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(content.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generic_create.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(response_json(generic_create).await["errors"]["kind"][0]
        .as_str()
        .unwrap()
        .contains("/api/admin/v1/news"));

    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/news")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "dedicated-news-create-boundary")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "content": content,
                        "category": "Company",
                        "authorDisplayName": "AIRTEKPOWER",
                        "coverMediaId": null,
                        "publishedAt": null,
                        "featured": false,
                        "dataClass": "editorial"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let id = created["content"]["id"].as_str().unwrap();

    let generic_list = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/content?limit=100")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generic_list.status(), StatusCode::OK);
    assert!(response_json(generic_list).await["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["kind"] != "news"));

    let mut generic_update = created["content"].clone();
    generic_update["kind"] = json!("article");
    generic_update.as_object_mut().unwrap().retain(|key, _| {
        matches!(
            key.as_str(),
            "kind" | "slug" | "locale" | "title" | "summary" | "body" | "seo" | "isPlaceholder"
        )
    });
    let update = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "generic-news-rejected-update")
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(generic_update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::CONFLICT);
    assert!(response_json(update).await["detail"]
        .as_str()
        .unwrap()
        .contains("/api/admin/v1/news"));

    for (path, body, key) in [
        (
            format!("/api/admin/v1/content/{id}/publish"),
            None,
            "generic-news-rejected-publish",
        ),
        (
            format!("/api/admin/v1/content/{id}/rollback"),
            Some(json!({"revision": 1, "reason": "Use the dedicated News workflow"})),
            "generic-news-rejected-rollback",
        ),
    ] {
        let has_body = body.is_some();
        let mut request = Request::post(path)
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", key)
            .header(header::IF_MATCH, "\"revision-1\"")
            .body(Body::from(
                body.map_or_else(String::new, |value| value.to_string()),
            ))
            .unwrap();
        if has_body {
            request
                .headers_mut()
                .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        }
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(response_json(response).await["detail"]
            .as_str()
            .unwrap()
            .contains("/api/admin/v1/news"));
    }
}

#[tokio::test]
async fn signed_content_preview_is_exact_short_lived_private_and_audited() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let original = json!({
        "kind": "article",
        "slug": "private-preview",
        "locale": "en",
        "title": "First private draft",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "First revision"}]}]}},
        "seo": {"indexable": true, "canonicalPath": "/en/resources/articles/private-preview"},
        "isPlaceholder": false
    });
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "preview-content-create-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(original.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut second = original.clone();
    second["title"] = json!("Second private draft");
    let updated = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::IF_MATCH, "\"revision-1\"")
                .header("idempotency-key", "preview-update-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(second.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let issue = |revision: i64, if_match: Option<&str>, expires_in_seconds: u32| {
        let mut request = Request::post(format!("/api/admin/v1/content/{id}/preview"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "revision": revision,
                    "expiresInSeconds": expires_in_seconds
                })
                .to_string(),
            ))
            .unwrap();
        if let Some(if_match) = if_match {
            request
                .headers_mut()
                .insert(header::IF_MATCH, if_match.parse().unwrap());
        }
        request
    };

    let stale = app
        .clone()
        .oneshot(issue(2, Some("\"revision-1\""), 600))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let missing_revision = app.clone().oneshot(issue(99, None, 600)).await.unwrap();
    assert_eq!(missing_revision.status(), StatusCode::NOT_FOUND);

    let issued = app
        .clone()
        .oneshot(issue(2, Some("\"revision-2\""), 600))
        .await
        .unwrap();
    assert_eq!(issued.status(), StatusCode::CREATED);
    assert_eq!(
        issued.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let location = issued
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(location.starts_with("http://localhost:3000/en/preview?token=v1."));
    let issued_body = response_json(issued).await;
    assert_eq!(issued_body["url"], location);
    assert_eq!(issued_body["revision"], 2);
    let token = location.split_once("?token=").unwrap().1.to_owned();

    let preview = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(
        preview.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    assert_eq!(
        preview.headers().get("x-robots-tag").unwrap(),
        "noindex, nofollow, noarchive"
    );
    let preview_body = response_json(preview).await;
    assert_eq!(preview_body["content"]["title"], "Second private draft");
    assert_eq!(preview_body["content"]["currentRevision"], 2);
    assert_eq!(preview_body["content"]["status"], "draft");

    let unavailable_published_fallback = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content/articles/private-preview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        unavailable_published_fallback.status(),
        StatusCode::NOT_FOUND
    );

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        unauthenticated
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap(),
        "private, no-store, max-age=0"
    );

    let mut tampered = token.into_bytes();
    let index = tampered.len() / 2;
    tampered[index] = if tampered[index] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(tampered).unwrap();
    let rejected = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {tampered}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::NOT_FOUND);

    let first_revision = app.clone().oneshot(issue(1, None, 600)).await.unwrap();
    let first_location = first_revision
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let first_token = first_location.split_once("?token=").unwrap().1;
    let exact_first = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(exact_first.status(), StatusCode::OK);
    assert_eq!(
        response_json(exact_first).await["content"]["title"],
        "First private draft"
    );

    state
        .data
        .write()
        .await
        .admin_sessions
        .values_mut()
        .next()
        .expect("issuing Admin session")
        .revoked = true;
    let revoked_session_preview = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked_session_preview.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        revoked_session_preview
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap(),
        "private, no-store, max-age=0"
    );

    let audit = state.data.read().await.audit_events.clone();
    let preview_audits: Vec<_> = audit
        .iter()
        .filter(|event| event.action == "content.preview.issue")
        .collect();
    assert_eq!(preview_audits.len(), 2);
    let serialized_audit = serde_json::to_string(&preview_audits).unwrap();
    assert!(!serialized_audit.contains("?token="));
    assert!(!serialized_audit.contains("v1."));
}

#[tokio::test]
async fn expired_content_preview_returns_gone_without_fallback() {
    let state = AppState::for_test();
    let content_id = uuid::Uuid::new_v4();
    let content = airtek_platform::models::ContentEntry {
        id: content_id,
        kind: airtek_platform::models::ContentKind::Article,
        slug: "expiring-preview".into(),
        locale: "en".into(),
        title: "Expiring preview".into(),
        summary: None,
        body: airtek_platform::models::RichTextDocument {
            schema_version: 1,
            doc: json!({"type": "doc", "content": []}),
        },
        seo: airtek_platform::models::SeoMetadata::default(),
        status: PublicationStatus::Draft,
        is_placeholder: false,
        current_revision: 1,
        published_revision: None,
        scheduled_for: None,
        updated_at: chrono::Utc::now(),
    };
    state
        .data
        .write()
        .await
        .content_revisions
        .entry(content_id)
        .or_default()
        .insert(1, content);
    let key = state.config.preview_signing_key.as_ref().unwrap();
    let token = airtek_platform::preview_token::issue(
        key,
        content_id,
        1,
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        1,
    )
    .unwrap()
    .token;
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;

    let response = build_router(state)
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::GONE);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
}

#[tokio::test]
async fn rich_text_entity_block_kind_is_allowlisted() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let request = |kind: &str| {
        Request::post("/api/admin/v1/content")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", format!("rich-text-{kind}-0001"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "kind": "article",
                    "slug": format!("entity-{kind}"),
                    "locale": "en",
                    "title": "Entity block",
                    "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": [{"type": "entityBlock", "attrs": {"kind": kind}}]}},
                    "seo": {"indexable": false},
                    "isPlaceholder": true
                })
                .to_string(),
            ))
            .unwrap()
    };
    let allowed = app
        .clone()
        .oneshot(request("related-product"))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::CREATED);
    let rejected = app.oneshot(request("raw-html")).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn login_failures_are_rate_limited_by_source_and_account() {
    let app = build_router(AppState::for_test());
    let _ = setup_admin(&app).await;
    let request = || {
        Request::post("/api/admin/v1/auth/login")
            .header(header::ORIGIN, "http://localhost:3100")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-real-ip", "192.0.2.10")
            .body(Body::from(
                json!({
                    "email": "admin@example.com",
                    "password": "definitely-not-correct"
                })
                .to_string(),
            ))
            .unwrap()
    };
    for _ in 0..5 {
        let response = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let blocked = app.oneshot(request()).await.unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn high_risk_operations_fail_closed_until_totp_is_enabled() {
    let app = build_router(AppState::for_test());
    let session = setup_admin_without_totp(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/admin/v1/operations")
                .header(header::COOKIE, session.cookie)
                .header("x-csrf-token", session.csrf)
                .header("x-totp-code", "123456")
                .header("idempotency-key", "operation-fail-closed-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "kind": "backup",
                        "reason": "Create verified backup before deployment",
                        "confirmation": "CREATE BACKUP"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response_json(response).await["detail"]
        .as_str()
        .unwrap()
        .contains("enable TOTP"));
}

#[tokio::test]
async fn in_memory_feishu_sync_fails_explicitly_without_a_provider() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let started = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/feishu/sync-runs")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "feishu-sync-contract-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"dryRun": true, "mappingVersion": "validated-mapping-v1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::ACCEPTED);
    let started = response_json(started).await;
    assert_eq!(started["status"], "failed");
    assert!(started["error"]
        .as_str()
        .unwrap()
        .contains("provider adapter"));

    let listed = app
        .oneshot(
            Request::get("/api/admin/v1/feishu/sync-runs")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    assert_eq!(listed["items"][0]["status"], "failed");
}

#[tokio::test]
async fn sync_and_background_operation_replays_preserve_ids_and_location() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;

    let sync_body = json!({"dryRun": true, "mappingVersion": "idempotency-v1"}).to_string();
    let sync = |body: String| {
        Request::post("/api/admin/v1/feishu/sync-runs")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "sync-replay-contract-0001")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    };
    let first_sync = app.clone().oneshot(sync(sync_body.clone())).await.unwrap();
    assert_eq!(first_sync.status(), StatusCode::ACCEPTED);
    let first_sync_body = response_json(first_sync).await;
    let replay_sync = app.clone().oneshot(sync(sync_body.clone())).await.unwrap();
    assert_eq!(replay_sync.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(replay_sync).await, first_sync_body);
    let different_sync = app
        .clone()
        .oneshot(sync(sync_body.replace("idempotency-v1", "idempotency-v2")))
        .await
        .unwrap();
    assert_eq!(different_sync.status(), StatusCode::CONFLICT);

    let operation_body = json!({
        "kind": "searchReindex",
        "reason": "Rebuild the validated public search projection",
        "confirmation": "REBUILD SEARCH INDEX"
    })
    .to_string();
    let operation = |body: String| {
        Request::post("/api/admin/v1/operations")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "operation-replay-contract-0001")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    };
    let first_operation = app
        .clone()
        .oneshot(operation(operation_body.clone()))
        .await
        .unwrap();
    assert_eq!(first_operation.status(), StatusCode::ACCEPTED);
    let first_location = first_operation
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .clone();
    let first_operation_body = response_json(first_operation).await;
    let replay_operation = app
        .clone()
        .oneshot(operation(operation_body.clone()))
        .await
        .unwrap();
    assert_eq!(replay_operation.status(), StatusCode::ACCEPTED);
    assert_eq!(
        replay_operation.headers().get(header::LOCATION).unwrap(),
        &first_location
    );
    assert_eq!(response_json(replay_operation).await, first_operation_body);
    let different_operation = app
        .clone()
        .oneshot(operation(operation_body.replace(
            "Rebuild the validated public search projection",
            "Use a different validated background task reason",
        )))
        .await
        .unwrap();
    assert_eq!(different_operation.status(), StatusCode::CONFLICT);

    let data = state.data.read().await;
    assert_eq!(data.sync_runs.len(), 1);
    assert_eq!(data.operations.len(), 1);
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "feishu.sync.queue")
            .count(),
        1
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "operation.queue")
            .count(),
        1
    );
}

#[tokio::test]
async fn in_memory_analytics_summary_uses_first_party_counts() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let summary = app
        .oneshot(
            Request::get("/api/admin/v1/analytics/summary")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(summary.status(), StatusCode::OK);
    let summary = response_json(summary).await;
    assert_eq!(summary["acceptedEventCount"], 0);
    assert_eq!(summary["rfqCount"], 0);
    assert_eq!(summary["contactCount"], 0);
    assert_eq!(summary["containsPii"], false);
    assert_eq!(summary["source"], "firstParty");
}

#[tokio::test]
async fn rfq_lists_redact_pii_without_the_dedicated_permission() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "pii-redaction-contract")
                .body(Body::from(
                    json!({
                        "journey": "selection",
                        "contact": {
                            "name": "Protected Buyer",
                            "email": "protected@example.com",
                            "phone": "+1 555 0100",
                            "company": "Example Industry"
                        },
                        "sourcePath": "/en/request-a-quote/selection",
                        "locale": "en",
                        "consent": true,
                        "context": {
                            "application": "Industrial cooling",
                            "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    for user in state.data.write().await.admin_users.values_mut() {
        user.permissions
            .retain(|permission| permission != "rfq.read_pii");
    }
    let listed = app
        .oneshot(
            Request::get("/api/admin/v1/rfqs")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    assert_eq!(
        listed["items"][0]["request"]["contact"]["name"],
        "[restricted]"
    );
    assert_eq!(
        listed["items"][0]["request"]["contact"]["email"],
        "[restricted]"
    );
    assert!(listed["items"][0]["request"]["contact"]["phone"].is_null());
    let stored = state.data.read().await;
    let stored = stored.rfqs.values().next().unwrap();
    assert_eq!(stored.request.contact.name, "Protected Buyer");
    assert_eq!(stored.request.contact.email, "protected@example.com");
}

#[tokio::test]
async fn totp_recovery_codes_and_session_revocation_are_end_to_end_enforced() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let setup_session = setup_admin_without_totp(&app).await;

    let enrollment = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/totp/enrollment")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", &setup_session.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enrollment.status(), StatusCode::OK);
    assert_eq!(
        enrollment.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );
    let enrollment = response_json(enrollment).await;
    assert_eq!(enrollment["algorithm"], "SHA1");
    assert_eq!(enrollment["digits"], 6);
    let totp = totp_rs::TOTP::from_url(enrollment["otpAuthUri"].as_str().unwrap())
        .expect("valid provisioning URI");
    let current_code = totp.generate_current().expect("system clock");

    let confirmed = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/totp/confirm")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", &setup_session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"code": current_code}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.status(), StatusCode::OK);
    assert_eq!(
        confirmed.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );
    let confirmed = response_json(confirmed).await;
    let recovery_code = confirmed["recoveryCodes"][0].as_str().unwrap().to_owned();
    assert_eq!(confirmed["recoveryCodes"].as_array().unwrap().len(), 10);

    let refreshed = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::COOKIE, &setup_session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);
    let refreshed_csrf = refreshed
        .headers()
        .get("x-csrf-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let refreshed_body = response_json(refreshed).await;
    assert_eq!(refreshed_body["totpEnabled"], true);

    let high_risk = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/operations")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", &refreshed_csrf)
                .header("x-totp-code", totp.generate_current().unwrap())
                .header("idempotency-key", "totp-high-risk-contract")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "kind": "backup",
                        "reason": "Verify a real TOTP reauthentication contract",
                        "confirmation": "CREATE BACKUP"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(high_risk.status(), StatusCode::ACCEPTED);

    let logged_out = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", refreshed_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);

    let login = |otp: String| {
        Request::post("/api/admin/v1/auth/login")
            .header(header::ORIGIN, "http://localhost:3100")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "email": "admin@example.com",
                    "password": "correct-horse-123",
                    "otp": otp
                })
                .to_string(),
            ))
            .unwrap()
    };

    let totp_login = app
        .clone()
        .oneshot(login(totp.generate_current().unwrap()))
        .await
        .unwrap();
    assert_eq!(totp_login.status(), StatusCode::OK);
    let totp_session = session_credentials(&totp_login);

    let recovery_login = app
        .clone()
        .oneshot(login(recovery_code.clone()))
        .await
        .unwrap();
    assert_eq!(recovery_login.status(), StatusCode::OK);
    let recovery_session = session_credentials(&recovery_login);

    let reused = app.clone().oneshot(login(recovery_code)).await.unwrap();
    assert_eq!(reused.status(), StatusCode::UNAUTHORIZED);

    let sessions = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/sessions")
                .header(header::COOKIE, &recovery_session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sessions.status(), StatusCode::OK);
    let sessions = response_json(sessions).await;
    let other_id = sessions
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["current"] == false)
        .and_then(|session| session["id"].as_str())
        .expect("another active session")
        .to_owned();

    let revoked = app
        .clone()
        .oneshot(
            Request::delete(format!("/api/admin/v1/auth/sessions/{other_id}"))
                .header(header::COOKIE, &recovery_session.cookie)
                .header("x-csrf-token", &recovery_session.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let revoked_session = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::COOKIE, totp_session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked_session.status(), StatusCode::UNAUTHORIZED);

    let audit = state.data.read().await.audit_events.clone();
    assert!(audit
        .iter()
        .any(|event| event.action == "auth.totp.enabled"));
    assert!(audit
        .iter()
        .any(|event| event.action == "auth.session.revoked"));
    assert!(audit.iter().all(|event| {
        let serialized = serde_json::to_string(event).unwrap();
        !serialized.contains(enrollment["secret"].as_str().unwrap_or_default())
            && !serialized.contains(confirmed["recoveryCodes"][0].as_str().unwrap_or_default())
    }));
}
