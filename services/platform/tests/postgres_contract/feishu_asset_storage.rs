use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use airtek_platform::{
    config::FeishuCredentials,
    services::feishu::{
        compensate_source_assets, store_source_assets, token_hash, SourceAttachment,
    },
};
use axum::{
    extract::Path,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use super::*;

const PDF: &[u8] = b"%PDF-1.7\nAIRTEK integration attachment";

async fn token() -> Json<Value> {
    Json(json!({
        "code": 0,
        "msg": "ok",
        "tenant_access_token": "integration-token",
        "expire": 7200
    }))
}

async fn download(
    Path(token): Path<String>,
    Extension(calls): Extension<Arc<AtomicUsize>>,
) -> Response {
    calls.fetch_add(1, Ordering::SeqCst);
    if token.starts_with("bad-") {
        return ([((header::CONTENT_TYPE), "text/html")], "not a pdf").into_response();
    }
    ([((header::CONTENT_TYPE), "application/pdf")], PDF).into_response()
}

async fn mock_feishu() -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route(
            "/open-apis/auth/v3/tenant_access_token/internal",
            post(token),
        )
        .route("/open-apis/drive/v1/medias/{token}/download", get(download))
        .layer(Extension(calls.clone()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), calls, handle)
}

fn attachment(token: String) -> SourceAttachment {
    SourceAttachment {
        file_token: token,
        original_name: "AX-200-drawing.pdf".into(),
        declared_size: Some(PDF.len() as u64),
        declared_media_type: Some("application/pdf".into()),
        source_field_id: "fld-drawing".into(),
        source_field_name: "产品图纸".into(),
        usage: "drawing".into(),
        source_revision: "revision-1".into(),
    }
}

fn object_key(prefix: &str, token: &str) -> String {
    let token_hash = token_hash(token);
    let checksum = format!("{:x}", Sha256::digest(PDF));
    format!(
        "{prefix}/feishu/{}/{}-{}.pdf",
        &token_hash[..2],
        token_hash,
        checksum
    )
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and AIRTEK_TEST_S3_ENDPOINT MinIO"]
async fn feishu_assets_deduplicate_download_publicly_and_compensate_objects() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let endpoint = std::env::var("AIRTEK_TEST_S3_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:19000".into());
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let suffix = Uuid::new_v4().simple().to_string();
    let prefix = format!("media/feishu-integration-{suffix}");
    let public_base = format!("{endpoint}/airtek-media");
    sqlx::query(
        r#"INSERT INTO object_storage_settings
           (singleton,provider,endpoint,region,bucket,access_key_id,secret_access_key,
            key_prefix,path_style,public_base_url,updated_by)
           VALUES (true,'s3',$1,'us-east-1','airtek-media','airtek-media-api',
                   'local-api-media-only',$2,true,$3,'integration-test')"#,
    )
    .bind(&endpoint)
    .bind(&prefix)
    .bind(&public_base)
    .execute(sandbox.pool())
    .await
    .unwrap();

    let (feishu_url, download_calls, server) = mock_feishu().await;
    let mut config = Config::for_test();
    config.database_url = Some(sandbox.connection_url().into());
    config.feishu = Some(FeishuCredentials::new("app".into(), "secret".into()));
    config.feishu_api_base_url = feishu_url;
    let state = AppState::new(config).unwrap();
    let connector_id: Uuid =
        sqlx::query_scalar("SELECT connector_id FROM feishu_connector_settings LIMIT 1")
            .fetch_one(&state.pool)
            .await
            .unwrap();

    let first_token = format!("first-{suffix}");
    let second_token = format!("second-{suffix}");
    let first = store_source_assets(&state, connector_id, &[attachment(first_token.clone())])
        .await
        .unwrap();
    assert!(first[0].newly_created);
    let replay = store_source_assets(&state, connector_id, &[attachment(first_token.clone())])
        .await
        .unwrap();
    assert!(!replay[0].newly_created);
    let checksum_reuse =
        store_source_assets(&state, connector_id, &[attachment(second_token.clone())])
            .await
            .unwrap();
    assert_eq!(first[0].media_asset_id, replay[0].media_asset_id);
    assert_eq!(first[0].media_asset_id, checksum_reuse[0].media_asset_id);
    assert_eq!(download_calls.load(Ordering::SeqCst), 2);
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM media_assets),(SELECT count(*) FROM feishu_asset_bindings)",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 2));

    let app = build_router(state.clone());
    let response = app
        .oneshot(
            Request::get(format!(
                "/api/public/v1/media/{}/download",
                first[0].media_asset_id
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(
        response.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"AX-200-drawing.pdf\""
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        PDF
    );

    let first_key = object_key(&prefix, &first_token);
    compensate_source_assets(&state, &first).await;
    assert_eq!(
        reqwest::get(format!("{public_base}/{first_key}"))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM media_assets")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    let third_token = format!("third-{suffix}");
    let bad_token = format!("bad-{suffix}");
    let failure = store_source_assets(
        &state,
        connector_id,
        &[attachment(third_token.clone()), attachment(bad_token)],
    )
    .await;
    assert!(failure.is_err());
    let third_key = object_key(&prefix, &third_token);
    assert_eq!(
        reqwest::get(format!("{public_base}/{third_key}"))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM media_assets")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);

    state.pool.close().await;
    server.abort();
    sandbox.cleanup().await;
}
