use airtek_platform::{
    models::{CursorPage, GuestSourceDaily, GuestVisit, GuestVisitAggregate},
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
fn analytics_openapi_exposes_runtime_cursor_and_limit_parameters() {
    let document = airtek_platform::openapi::document();
    for path in [
        "/api/admin/v1/analytics/visits",
        "/api/admin/v1/analytics/sources",
    ] {
        let parameters = document["paths"][path]["get"]["parameters"]
            .as_array()
            .expect("analytics query parameters");
        for name in ["from", "to", "cursor", "limit"] {
            assert!(
                parameters
                    .iter()
                    .any(|parameter| { parameter["name"] == name && parameter["in"] == "query" }),
                "{path} must declare the {name} runtime query parameter"
            );
        }
        let limit = parameters
            .iter()
            .find(|parameter| parameter["name"] == "limit")
            .unwrap();
        assert_eq!(limit["schema"]["type"], "integer");
        assert_eq!(limit["schema"]["minimum"], 1);
        assert_eq!(limit["schema"]["maximum"], 100);
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
async fn analytics_http_uses_opaque_scope_bound_keyset_cursors() {
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

    let first_response = memory_request(&state, "/analytics/visits?limit=2").await;
    if first_response.status() != StatusCode::OK {
        let status = first_response.status();
        let bytes = first_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        panic!(
            "unexpected analytics status {status}: {}",
            String::from_utf8_lossy(&bytes)
        );
    }
    let first: CursorPage<GuestVisitAggregate> = response_json(first_response).await;
    assert_eq!(first.items.len(), 2);
    let cursor = first.next_cursor.expect("second page cursor");
    assert!(
        !cursor.contains(&prefix),
        "cursor must not expose dimensions"
    );

    let second_response = memory_request(
        &state,
        &format!("/analytics/visits?limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second: CursorPage<GuestVisitAggregate> = response_json(second_response).await;
    assert_eq!(second.items.len(), 2);
    assert!(second.next_cursor.is_none());
    let first_paths = first
        .items
        .iter()
        .map(|item| &item.landing_path)
        .collect::<Vec<_>>();
    assert!(second
        .items
        .iter()
        .all(|item| !first_paths.contains(&&item.landing_path)));

    let changed_scope = memory_request(
        &state,
        &format!("/analytics/visits?limit=2&from=2026-09-01T00:00:00Z&cursor={cursor}"),
    )
    .await;
    assert_eq!(changed_scope.status(), StatusCode::BAD_REQUEST);
    let problem: Value = response_json(changed_scope).await;
    assert_eq!(problem["status"], 400);

    let wrong_endpoint = memory_request(
        &state,
        &format!("/analytics/sources?limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(wrong_endpoint.status(), StatusCode::BAD_REQUEST);

    let source_response = memory_request(&state, "/analytics/sources?limit=1").await;
    assert_eq!(source_response.status(), StatusCode::OK);
    let sources: CursorPage<GuestSourceDaily> = response_json(source_response).await;
    assert_eq!(sources.items.len(), 1);
    assert!(sources.next_cursor.is_some());
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn postgres_analytics_pagination_merges_history_and_raw_without_overlap() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
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
    .expect("historical aggregate fixture");
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
    .expect("consent fixture");
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
    .expect("raw visit fixture");

    for offset in 0..2 {
        let other_path = format!("/en/analytics-pagination-{prefix}-other-{offset}");
        sqlx::query(
            r#"INSERT INTO guest_source_daily
                   (bucket_date,source_type,source_name,utm_source,utm_medium,utm_campaign,
                    landing_path,locale,visits)
               VALUES ((($1 AT TIME ZONE 'UTC')::date - $2::integer),'direct','','','','',
                       $3,'en',1)"#,
        )
        .bind(at)
        .bind(offset)
        .bind(other_path)
        .execute(&pool)
        .await
        .expect("additional aggregate fixture");
    }

    let mut config = Config::for_test();
    config.database_url = Some(database_url.clone());
    let state = AppState::new(config).expect("PostgreSQL state");
    let first_response = memory_request(&state, "/analytics/visits?limit=1").await;
    assert_eq!(first_response.status(), StatusCode::OK);
    let first: CursorPage<GuestVisitAggregate> = response_json(first_response).await;
    let cursor = first.next_cursor.clone().expect("next page cursor");
    let second_response = memory_request(
        &state,
        &format!("/analytics/visits?limit=1&cursor={cursor}"),
    )
    .await;
    assert_eq!(second_response.status(), StatusCode::OK);
    let second: CursorPage<GuestVisitAggregate> = response_json(second_response).await;
    assert_ne!(first.items[0].landing_path, second.items[0].landing_path);

    let all_response = memory_request(&state, "/analytics/visits?limit=100").await;
    let all: CursorPage<GuestVisitAggregate> = response_json(all_response).await;
    let merged = all
        .items
        .iter()
        .find(|item| item.landing_path == landing_path)
        .expect("merged historical and raw row");
    assert_eq!(merged.visits, 5);
    assert_eq!(merged.page_views, 5);

    let first_source_response = memory_request(&state, "/analytics/sources?limit=1").await;
    assert_eq!(first_source_response.status(), StatusCode::OK);
    let first_source: CursorPage<GuestSourceDaily> = response_json(first_source_response).await;
    let source_cursor = first_source.next_cursor.expect("source next page cursor");
    let second_source_response = memory_request(
        &state,
        &format!("/analytics/sources?limit=1&cursor={source_cursor}"),
    )
    .await;
    assert_eq!(second_source_response.status(), StatusCode::OK);
    let second_source: CursorPage<GuestSourceDaily> = response_json(second_source_response).await;
    assert_ne!(
        first_source.items[0].landing_path,
        second_source.items[0].landing_path
    );
    let all_sources_response = memory_request(&state, "/analytics/sources?limit=100").await;
    let all_sources: CursorPage<GuestSourceDaily> = response_json(all_sources_response).await;
    let merged_source = all_sources
        .items
        .iter()
        .find(|item| item.landing_path == landing_path)
        .expect("merged historical and raw source row");
    assert_eq!(merged_source.visits, 5);
    assert_eq!(merged_source.page_views, 5);

    sqlx::query("DELETE FROM guest_source_daily WHERE landing_path LIKE $1")
        .bind(format!("/en/analytics-pagination-{prefix}%"))
        .execute(&pool)
        .await
        .expect("aggregate cleanup");
    sqlx::query("DELETE FROM guest_visits WHERE id=$1")
        .bind(visit_id)
        .execute(&pool)
        .await
        .expect("visit cleanup");
    sqlx::query("DELETE FROM consent_records WHERE id=$1")
        .bind(consent_id)
        .execute(&pool)
        .await
        .expect("consent cleanup");
}
