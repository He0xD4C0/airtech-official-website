use axum::{
    extract::{OriginalUri, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::Utc;

use crate::{error::ApiError, models::HealthStatus, openapi, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness))
        .route("/internal/metrics", get(metrics))
        .route("/openapi.json", get(openapi_json))
        .route("/robots.txt", get(robots))
}

async fn liveness(State(state): State<AppState>) -> Json<HealthStatus> {
    Json(HealthStatus {
        status: "ok".into(),
        service: "airtek-platform-api".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        persistence: if state.pool.is_some() {
            "postgresqlConfigured".into()
        } else {
            "inMemory".into()
        },
        timestamp: Utc::now(),
    })
}

async fn readiness(State(state): State<AppState>) -> Result<Json<HealthStatus>, ApiError> {
    let persistence = state.check_persistence().await?;
    Ok(Json(HealthStatus {
        status: "ready".into(),
        service: "airtek-platform-api".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        persistence,
        timestamp: Utc::now(),
    }))
}

async fn openapi_json() -> Json<serde_json::Value> {
    Json(openapi::document())
}

async fn metrics(State(state): State<AppState>) -> Response {
    let mut response = (StatusCode::OK, state.request_metrics.render()).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/openmetrics-text; version=1.0.0; charset=utf-8"),
    );
    response
}

async fn robots() -> Response {
    let mut response = (StatusCode::OK, "User-agent: *\nDisallow: /\n").into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

pub async fn not_found(OriginalUri(uri): OriginalUri) -> ApiError {
    ApiError::not_found("The API route does not exist.").with_instance(uri.to_string())
}
