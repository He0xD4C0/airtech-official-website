use std::net::{Ipv6Addr, SocketAddr};

#[cfg(feature = "devtools")]
use airtek_platform::services::development_seed;
use airtek_platform::{
    auth::AdminPrincipal,
    build_router,
    models::{
        ContentEntry, ContentKind, Product, ProductFamily, PublicationStatus, RichTextDocument,
        SeoMetadata, SyncRun, SyncRunStatus,
    },
    AppState, Config,
};
use axum::{
    body::Body,
    extract::{connect_info::ConnectInfo, Extension},
    http::{header, Request, StatusCode},
};
#[cfg(feature = "devtools")]
use axum::{http::Method, response::Response, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
#[cfg(feature = "devtools")]
use sqlx::Row;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

fn contact_request(index: usize, run_id: Uuid, peer: SocketAddr) -> Request<Body> {
    let mut request = Request::post("/api/public/v1/contact")
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            "idempotency-key",
            format!("postgres-rate-{run_id}-{index:04}"),
        )
        .header("x-forwarded-for", format!("203.0.113.{}", index + 1))
        .body(Body::from(
            json!({
                "contact": {"name": "Persistence Test", "email": "test@example.com"},
                "topic": "Rate-limit persistence",
                "message": "This record verifies durable public rate limiting.",
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

fn postgres_state(database_url: &str) -> AppState {
    let mut config = Config::for_test();
    config.database_url = Some(database_url.to_owned());
    AppState::new(config).expect("PostgreSQL test state")
}

#[cfg(feature = "devtools")]
async fn direct_admin_mutation(
    app: &Router,
    method: Method,
    path: &str,
    revision: Option<i64>,
    idempotency_key: &str,
    body: Option<Value>,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("idempotency-key", idempotency_key)
        .header("x-actor", "postgres-contract-editor");
    if let Some(revision) = revision {
        builder = builder.header(header::IF_MATCH, format!("\"revision-{revision}\""));
    }
    let request = if let Some(body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    app.clone().oneshot(request).await.unwrap()
}

#[cfg(feature = "devtools")]
fn editorial_content_draft(payload: &Value, title: &str) -> Value {
    let mut seo = payload["seo"].clone();
    seo["indexable"] = json!(payload["seo"]["canonicalPath"].is_string());
    json!({
        "kind": payload["kind"],
        "slug": payload["slug"],
        "locale": payload["locale"],
        "title": title,
        "summary": payload["summary"],
        "body": payload["body"],
        "seo": seo,
        "isPlaceholder": false
    })
}

include!("postgres_contract/bootstrap_and_fixture_takeover.rs");
include!("postgres_contract/preview_and_invitations.rs");
include!("postgres_contract/rate_limit_and_admin_idempotency.rs");
include!("postgres_contract/identity_mutation_atomicity.rs");
include!("postgres_contract/settings_and_product_publish.rs");
include!("postgres_contract/general_information.rs");
