use std::{
    net::{Ipv6Addr, SocketAddr},
    path::PathBuf,
};

#[cfg(feature = "devtools")]
use airtek_platform::services::development_seed;
use airtek_platform::{
    auth::AdminPrincipal,
    build_router,
    models::{Product, ProductFamily, PublicationStatus, SyncRun, SyncRunStatus},
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
#[cfg(feature = "devtools")]
use sqlx::postgres::PgPoolOptions;
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

fn postgres_direct_media_state(database_url: &str) -> AppState {
    use airtek_platform::services::media::{MediaStorageKind, MediaStorageSettings};

    let mut config = Config::for_test();
    config.database_url = Some(database_url.to_owned());
    config.media.storage = Some(MediaStorageSettings {
        kind: MediaStorageKind::S3,
        local_root: PathBuf::from("/unused"),
        endpoint: "https://s3.example.test".into(),
        region: "us-east-1".into(),
        bucket: "postgres-contract".into(),
        access_key_id: "test-access".into(),
        secret_access_key: "test-secret".into(),
        key_prefix: "media".into(),
        path_style: true,
    });
    AppState::new(config).expect("PostgreSQL direct-media test state")
}

#[cfg(feature = "devtools")]
async fn response_json(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

include!("postgres_contract/bootstrap_and_fixture_takeover.rs");
include!("postgres_contract/preview_and_invitations.rs");
include!("postgres_contract/rate_limit_and_admin_idempotency.rs");
include!("postgres_contract/identity_mutation_atomicity.rs");
include!("postgres_contract/settings_and_product_publish.rs");
include!("postgres_contract/unified_content.rs");
include!("postgres_contract/public_projection.rs");
include!("postgres_contract/cms_publication_lifecycle.rs");
include!("postgres_contract/cms_archive_lifecycle.rs");
include!("postgres_contract/cms_publication_concurrency.rs");
include!("postgres_contract/media_assets.rs");
include!("postgres_contract/cms_round_trip.rs");
