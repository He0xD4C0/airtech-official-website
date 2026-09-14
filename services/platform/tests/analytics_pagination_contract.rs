use airtek_platform::{
    models::{CursorPage, GuestSourceDaily, GuestVisit},
    routes::admin_data,
    AppState, Config,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

mod support;

#[test]
fn source_analytics_openapi_exposes_runtime_cursor_and_limit_parameters() {
    let document = airtek_platform::openapi::document();
    let parameters = document["paths"]["/api/admin/v1/analytics/sources"]["get"]["parameters"]
        .as_array()
        .expect("analytics query parameters");
    for name in ["from", "to", "cursor", "limit"] {
        assert!(parameters
            .iter()
            .any(|parameter| { parameter["name"] == name && parameter["in"] == "query" }));
    }
}

async fn response_json<T: DeserializeOwned>(response: axum::response::Response) -> T {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("JSON response")
}

async fn memory_request(state: &AppState, path: &str) -> axum::response::Response {
    admin_data::router()
        .with_state(state.clone())
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn visit(at: DateTime<Utc>, landing_path: String, source: String) -> GuestVisit {
    GuestVisit {
        id: Uuid::new_v4(),
        anonymous_session_id: Uuid::new_v4(),
        landing_path,
        referrer_domain: Some(format!("{source}.example.test")),
        source,
        medium: Some("referral".into()),
        campaign: None,
        first_seen_at: at,
        last_seen_at: at,
        retention_until: at + Duration::days(180),
    }
}

#[tokio::test]
async fn source_analytics_uses_opaque_scope_bound_keyset_cursors() {
    let state = AppState::new(Config::for_test()).unwrap();
    let prefix = Uuid::new_v4().simple().to_string();
    let base = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap();
    {
        let mut data = state.data.write().await;
        for offset in 0..4 {
            let item = visit(
                base - Duration::days(offset),
                format!("/en/analytics-page-{prefix}-{offset}"),
                format!("source-{offset}"),
            );
            data.guest_visits.insert(item.id, item);
        }
    }

    let first_response = memory_request(&state, "/analytics/sources?limit=2").await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first: CursorPage<GuestSourceDaily> = response_json(first_response).await;
    assert_eq!(first.items.len(), 2);
    let cursor = first.next_cursor.expect("second page cursor");
    assert!(
        !cursor.contains(&prefix),
        "cursor must not expose dimensions"
    );

    let second_response = memory_request(
        &state,
        &format!("/analytics/sources?limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second: CursorPage<GuestSourceDaily> = response_json(second_response).await;
    assert_eq!(second.items.len(), 2);
    assert!(second.next_cursor.is_none());

    let changed_scope = memory_request(
        &state,
        &format!("/analytics/sources?limit=2&from=2026-09-01T00:00:00Z&cursor={cursor}"),
    )
    .await;
    assert_eq!(changed_scope.status(), StatusCode::BAD_REQUEST);
    let problem: Value = response_json(changed_scope).await;
    assert_eq!(problem["status"], 400);
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn postgres_source_pagination_merges_history_and_raw_without_overlap() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .unwrap();
    support::assert_flyway_schema_current(&pool).await;

    let prefix = Uuid::new_v4().simple().to_string();
    let landing_path = format!("/en/analytics-pagination-{prefix}");
    let source_name = format!("{prefix}.example.test");
    let at = Utc::now() - Duration::days(2);
    let session_id = Uuid::new_v4();
    let consent_id = Uuid::new_v4();
    let visit_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO guest_source_daily
               (bucket_date,source_type,source_name,utm_source,utm_medium,utm_campaign,
                landing_path,locale,visits,page_views,rfq_starts,rfq_submissions)
           VALUES (($1 AT TIME ZONE 'UTC')::date,'referral',$2,'','referral','',
                   $3,'en',4,5,1,1)"#,
    )
    .bind(at)
    .bind(&source_name)
    .bind(&landing_path)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO consent_records
               (id,anonymous_session_id,policy_version,analytics_allowed,granted_at)
           VALUES ($1,$2,'analytics-pagination-v1',true,$3)"#,
    )
    .bind(consent_id)
    .bind(session_id)
    .bind(at)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO guest_visits
               (id,anonymous_session_id,consent_record_id,consent_analytics_allowed,
                locale,landing_path,source_type,referrer_host,utm_source,utm_medium,
                utm_campaign,first_seen_at,last_seen_at,retention_until,created_at)
           VALUES ($1,$2,$3,true,'en',$4,'referral',$5,NULL,'referral',NULL,
                   $6,$6,$7,$6)"#,
    )
    .bind(visit_id)
    .bind(session_id)
    .bind(consent_id)
    .bind(&landing_path)
    .bind(&source_name)
    .bind(at)
    .bind(at + Duration::days(180))
    .execute(&pool)
    .await
    .unwrap();

    let mut config = Config::for_test();
    config.database_url = Some(database_url.clone());
    let state = AppState::new(config).unwrap();
    let response = memory_request(&state, "/analytics/sources?limit=100").await;
    let page: CursorPage<GuestSourceDaily> = response_json(response).await;
    let merged = page
        .items
        .iter()
        .find(|item| item.landing_path == landing_path)
        .unwrap();
    assert_eq!(merged.visits, 5);
    assert_eq!(merged.page_views, 5);

    sqlx::query("DELETE FROM guest_source_daily WHERE landing_path=$1")
        .bind(&landing_path)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM guest_visits WHERE id=$1")
        .bind(visit_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM consent_records WHERE id=$1")
        .bind(consent_id)
        .execute(&pool)
        .await
        .unwrap();
}
