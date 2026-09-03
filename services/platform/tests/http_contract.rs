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

include!("http_contract/bootstrap_and_public_boundaries.rs");
include!("http_contract/product_and_submission_contracts.rs");
include!("http_contract/settings_and_rate_limits.rs");
include!("http_contract/analytics_consent.rs");
include!("http_contract/rfq_and_content_concurrency.rs");
include!("http_contract/publication_and_identity.rs");
include!("http_contract/editorial_history.rs");
include!("http_contract/content_preview.rs");
include!("http_contract/operations_security.rs");
include!("http_contract/operations_and_recovery.rs");
