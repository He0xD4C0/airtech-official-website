use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::config::{Config, FeishuCredentials};

use super::FeishuClient;

#[derive(Clone, Default)]
struct MockState {
    token_calls: Arc<AtomicUsize>,
    field_calls: Arc<AtomicUsize>,
    record_calls: Arc<AtomicUsize>,
    retry_fields: Arc<AtomicBool>,
    reject_records: Arc<AtomicBool>,
    fail_first_token: Arc<AtomicBool>,
}

async fn token(State(state): State<MockState>) -> Response {
    let call = state.token_calls.fetch_add(1, Ordering::SeqCst);
    if call == 0 && state.fail_first_token.load(Ordering::SeqCst) {
        return (StatusCode::INTERNAL_SERVER_ERROR, "retry token").into_response();
    }
    Json(json!({
        "code": 0,
        "msg": "ok",
        "tenant_access_token": format!("token-{call}"),
        "expire": 7200
    }))
    .into_response()
}

async fn fields(
    State(state): State<MockState>,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    let call = state.field_calls.fetch_add(1, Ordering::SeqCst);
    if state.retry_fields.load(Ordering::SeqCst) {
        return match call {
            0 => StatusCode::UNAUTHORIZED.into_response(),
            1 => {
                let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("0"));
                response
            }
            2 => StatusCode::BAD_GATEWAY.into_response(),
            _ => field_page("fld-model", false, None),
        };
    }
    if query
        .get("page_token")
        .is_some_and(|value| value == "fields-2")
    {
        field_page("fld-file", false, None)
    } else {
        field_page("fld-model", true, Some("fields-2"))
    }
}

fn field_page(id: &str, has_more: bool, page_token: Option<&str>) -> Response {
    Json(json!({
        "code": 0,
        "msg": "ok",
        "data": {
            "items": [{
                "field_id": id,
                "field_name": if id == "fld-model" { "型号" } else { "附件" },
                "type": if id == "fld-model" { 1 } else { 17 },
                "is_primary": id == "fld-model"
            }],
            "has_more": has_more,
            "page_token": page_token
        }
    }))
    .into_response()
}

async fn records(
    State(state): State<MockState>,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    state.record_calls.fetch_add(1, Ordering::SeqCst);
    if state.reject_records.load(Ordering::SeqCst) {
        return Json(json!({"code": 1254302, "msg": "table unavailable"})).into_response();
    }
    let second = query
        .get("page_token")
        .is_some_and(|value| value == "records-2");
    Json(json!({
        "code": 0,
        "msg": "ok",
        "data": {
            "items": [{
                "record_id": if second { "rec-2" } else { "rec-1" },
                "fields": {"型号": if second { "AX-2" } else { "AX-1" }},
                "last_modified_time": if second { "2" } else { "1" }
            }],
            "has_more": !second,
            "page_token": if second { None::<&str> } else { Some("records-2") }
        }
    }))
    .into_response()
}

async fn download(Path(token): Path<String>) -> Response {
    if token == "missing" {
        return StatusCode::NOT_FOUND.into_response();
    }
    (
        [(header::CONTENT_TYPE, "application/pdf")],
        b"%PDF-1.7\nmock".to_vec(),
    )
        .into_response()
}

async fn mock_client(state: MockState) -> (FeishuClient, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route(
            "/open-apis/auth/v3/tenant_access_token/internal",
            post(token),
        )
        .route(
            "/open-apis/bitable/v1/apps/{app}/tables/{table}/fields",
            get(fields),
        )
        .route(
            "/open-apis/bitable/v1/apps/{app}/tables/{table}/records",
            get(records),
        )
        .route("/open-apis/drive/v1/medias/{token}/download", get(download))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock listener");
    let address = listener.local_addr().expect("mock address");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("mock server");
    });
    let mut config = Config::for_test();
    config.feishu = Some(FeishuCredentials::new("app".into(), "secret".into()));
    config.feishu_api_base_url = format!("http://{address}");
    (FeishuClient::new(&config), handle)
}

#[tokio::test]
async fn caches_tokens_and_paginates_fields_and_records() {
    let state = MockState::default();
    let counters = state.clone();
    let (client, server) = mock_client(state).await;

    let fields = client.list_fields("app", "table").await.unwrap();
    assert_eq!(fields.len(), 2);
    let first = client
        .list_records_page("app", "table", None)
        .await
        .unwrap();
    let second = client
        .list_records_page("app", "table", first.next_page_token.as_deref())
        .await
        .unwrap();
    assert_eq!(first.items[0].record_id, "rec-1");
    assert_eq!(second.items[0].record_id, "rec-2");
    assert_eq!(counters.token_calls.load(Ordering::SeqCst), 1);
    assert_eq!(counters.field_calls.load(Ordering::SeqCst), 2);
    assert_eq!(counters.record_calls.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn refreshes_once_after_401_and_retries_rate_limits_and_server_errors() {
    let state = MockState::default();
    state.retry_fields.store(true, Ordering::SeqCst);
    let counters = state.clone();
    let (client, server) = mock_client(state).await;

    let fields = client.list_fields("app", "table").await.unwrap();
    assert_eq!(fields[0].field_id, "fld-model");
    assert_eq!(counters.field_calls.load(Ordering::SeqCst), 4);
    assert_eq!(counters.token_calls.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn retries_a_transient_token_endpoint_failure() {
    let state = MockState::default();
    state.fail_first_token.store(true, Ordering::SeqCst);
    let counters = state.clone();
    let (client, server) = mock_client(state).await;

    client.test_token().await.unwrap();
    assert_eq!(counters.token_calls.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn treats_application_rejections_as_non_retryable_table_errors() {
    let state = MockState::default();
    state.reject_records.store(true, Ordering::SeqCst);
    let (client, server) = mock_client(state).await;

    let error = client
        .list_records_page("app", "table", None)
        .await
        .err()
        .unwrap();
    assert_eq!(error.status(), StatusCode::CONFLICT);
    server.abort();
}

#[tokio::test]
async fn downloads_and_hashes_attachments_but_rejects_oversized_content() {
    let (client, server) = mock_client(MockState::default()).await;

    let downloaded = client.download_asset("pdf").await.unwrap();
    assert_eq!(
        tokio::fs::read(downloaded.path()).await.unwrap(),
        b"%PDF-1.7\nmock"
    );
    assert_eq!(downloaded.byte_size, 13);
    assert_eq!(downloaded.content_type.as_deref(), Some("application/pdf"));
    assert_eq!(downloaded.sha256.len(), 64);
    let staged_path = downloaded.path().to_owned();
    drop(downloaded);
    assert!(!staged_path.exists());

    let error = super::validate_attachment_length(Some(100_u64 * 1024 * 1024 + 1))
        .err()
        .unwrap();
    assert_eq!(error.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let missing = client.download_asset("missing").await.err().unwrap();
    assert_eq!(missing.status(), StatusCode::CONFLICT);
    server.abort();
}
