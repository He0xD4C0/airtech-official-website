use airtek_platform::{
    models::{AnalyticsOverview, CursorPage, GuestSourceDaily},
    routes::{admin, admin_data},
    worker::apply_retention,
    AppState, Config,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{DateTime, Duration, Utc};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, Row};
use tower::ServiceExt;
use uuid::Uuid;

mod support;

fn postgres_state(database_url: &str) -> AppState {
    let mut config = Config::for_test();
    config.database_url = Some(database_url.to_owned());
    AppState::new(config).expect("PostgreSQL test state")
}

async fn analytics_page<T>(database_url: &str, path: &str) -> CursorPage<T>
where
    T: DeserializeOwned,
{
    let response = admin_data::router()
        .with_state(postgres_state(database_url))
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).expect("analytics response contract")
}

async fn analytics_rows(
    database_url: &str,
    landing_path: &str,
) -> (GuestSourceDaily, GuestSourceDaily) {
    let source = analytics_page::<GuestSourceDaily>(database_url, "/analytics/sources")
        .await
        .items
        .into_iter()
        .find(|row| row.landing_path == landing_path)
        .expect("landing path is visible in source analytics");
    (source.clone(), source)
}

async fn analytics_overview(
    database_url: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> AnalyticsOverview {
    let response = admin::router()
        .with_state(postgres_state(database_url))
        .oneshot(
            Request::get(format!(
                "/analytics/overview?from={}&to={}",
                from.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                to.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).expect("analytics overview contract")
}

#[path = "retention_contract/engagement_cutoff.rs"]
mod engagement_cutoff;
#[path = "retention_contract/materialization_and_deletion.rs"]
mod materialization_and_deletion;
