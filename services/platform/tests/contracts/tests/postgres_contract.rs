use std::{
    net::{Ipv6Addr, SocketAddr},
    path::PathBuf,
};

use airtek_domain::models::{Product, ProductFamily, PublicationStatus, SyncRun, SyncRunStatus};
use airtek_http::build_router;
use airtek_runtime::auth::AdminPrincipal;
use airtek_runtime::AppState;
use airtek_runtime::Config;
use axum::{
    body::Body,
    extract::{connect_info::ConnectInfo, Extension},
    http::{header, Request, StatusCode},
};
#[cfg(feature = "devtools")]
use axum::{http::Method, response::Response, Router};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use uuid::Uuid;

#[path = "postgres_contract/public_product_query.rs"]
mod public_product_query;
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
    use airtek_runtime::services::media::{MediaStorageKind, MediaStorageSettings};

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
        public_base_url: "https://media.example.test".into(),
    });
    AppState::new(config).expect("PostgreSQL direct-media test state")
}

#[cfg(feature = "devtools")]
async fn response_json(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[cfg(feature = "devtools")]
fn cms_principal() -> AdminPrincipal {
    AdminPrincipal {
        user_id: Uuid::new_v4(),
        display_name: "TEST ONLY CMS admin".into(),
        email: format!("cms-test-{}@example.com", Uuid::new_v4().simple()),
        role: "super-admin".into(),
        role_keys: vec!["super-admin".into()],
        permissions: [
            "content.read",
            "content.write",
            "content.publish",
            "media.write",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        session_id: Uuid::new_v4(),
        session_token_hash: vec![1; 32],
        csrf_hash: vec![2; 32],
        totp_enabled: true,
        must_change_password: false,
        must_confirm_recovery_key: false,
        phone_verified: false,
    }
}

#[cfg(feature = "devtools")]
async fn admin_get_json(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, response_json(response).await)
}

#[cfg(feature = "devtools")]
fn item_ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect()
}

#[path = "postgres_contract/admin_product_listing.rs"]
mod admin_product_listing;
#[path = "postgres_contract/admin_workflows.rs"]
mod admin_workflows;
#[path = "postgres_contract/cms_private_workflow.rs"]
mod cms_private_workflow;
#[cfg(feature = "devtools")]
#[path = "postgres_contract/development_admin.rs"]
mod development_admin;
#[cfg(feature = "devtools")]
#[path = "postgres_contract/development_public_site.rs"]
mod development_public_site;
#[path = "postgres_contract/feishu_archive_asset_storage.rs"]
mod feishu_archive_asset_storage;
#[path = "postgres_contract/feishu_asset_storage.rs"]
mod feishu_asset_storage;
#[path = "postgres_contract/feishu_reconciliation.rs"]
mod feishu_reconciliation;
#[path = "postgres_contract/feishu_settings.rs"]
mod feishu_settings;
#[path = "postgres_contract/feishu_sync.rs"]
mod feishu_sync;
#[path = "postgres_contract/identity_mutation_atomicity.rs"]
mod identity_mutation_atomicity;
#[path = "postgres_contract/media_assets.rs"]
mod media_assets;
#[path = "postgres_contract/object_storage_settings.rs"]
mod object_storage_settings;
#[path = "postgres_contract/rate_limit_and_admin_idempotency.rs"]
mod rate_limit_and_admin_idempotency;
#[path = "postgres_contract/settings_and_product_publish.rs"]
mod settings_and_product_publish;
