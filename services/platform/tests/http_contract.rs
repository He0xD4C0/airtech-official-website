use airtek_platform::{build_router, AppState, Config};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[test]
fn application_state_requires_postgresql_configuration() {
    let error = match AppState::new(Config::for_test()) {
        Ok(_) => panic!("a missing DATABASE_URL must fail closed"),
        Err(error) => error,
    };
    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn liveness_declares_the_single_persistence_boundary() {
    let mut config = Config::for_test();
    config.database_url = Some("postgresql://localhost/unused".into());
    let app = build_router(AppState::new(config).expect("valid PostgreSQL URL"));
    let response = app
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["persistence"], "postgresqlConfigured");
}
