use std::collections::BTreeMap;

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub instance: Option<String>,
    pub request_id: Uuid,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<String, Vec<String>>,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    problem: Box<ProblemDetails>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.problem.title, self.problem.detail)
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn new(status: StatusCode, title: &str, detail: impl Into<String>) -> Self {
        Self {
            status,
            problem: Box::new(ProblemDetails {
                problem_type: format!(
                    "https://api.airtekpower.example/problems/{}",
                    status.as_u16()
                ),
                title: title.into(),
                status: status.as_u16(),
                detail: detail.into(),
                instance: None,
                request_id: Uuid::new_v4(),
                errors: BTreeMap::new(),
            }),
        }
    }

    pub fn validation(errors: BTreeMap<String, Vec<String>>) -> Self {
        let mut value = Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Validation failed",
            "One or more request fields are invalid.",
        );
        value.problem.errors = errors;
        value
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "Bad request", detail)
    }

    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "Authentication required", detail)
    }

    pub fn forbidden(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "Forbidden", detail)
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "Not found", detail)
    }

    pub fn gone(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::GONE, "Gone", detail)
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "Conflict", detail)
    }

    pub fn too_many_requests(detail: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "Too many requests", detail)
    }

    pub fn precondition_required(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::PRECONDITION_REQUIRED,
            "Precondition required",
            detail,
        )
    }

    pub fn service_unavailable(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Service unavailable",
            detail,
        )
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error",
            detail,
        )
    }

    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.problem.instance = Some(instance.into());
        self
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(*self.problem)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(error = %error, "database operation failed");
        Self::service_unavailable("The persistence service is temporarily unavailable.")
    }
}

pub fn json_hash(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}
