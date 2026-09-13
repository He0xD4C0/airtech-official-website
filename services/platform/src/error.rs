use std::collections::BTreeMap;

use axum::{
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const MEDIA_IDEMPOTENCY_CONFLICT: &str = "media_idempotency_conflict";
pub const MEDIA_DECODE_FAILED: &str = "media_decode_failed";
pub const CONTENT_DEPENDENCY_CONFLICT: &str = "content_dependency_conflict";

pub const STABLE_DOMAIN_PROBLEM_CODES: [&str; 3] = [
    MEDIA_IDEMPOTENCY_CONFLICT,
    MEDIA_DECODE_FAILED,
    CONTENT_DEPENDENCY_CONFLICT,
];

pub fn problem_type_uri(code: &str) -> String {
    format!("https://api.airtekpower.example/problems/{code}")
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<Value>,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    problem: Box<ProblemDetails>,
    retry_after_seconds: Option<u32>,
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
                problem_type: problem_type_uri(&status.as_u16().to_string()),
                title: title.into(),
                status: status.as_u16(),
                detail: detail.into(),
                instance: None,
                request_id: Uuid::new_v4(),
                errors: BTreeMap::new(),
                issues: Vec::new(),
            }),
            retry_after_seconds: None,
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

    pub fn with_code(mut self, code: &str) -> Self {
        self.problem.problem_type = problem_type_uri(code);
        self
    }

    pub fn with_issues(mut self, issues: Vec<Value>) -> Self {
        self.problem.issues = issues;
        self
    }

    pub fn with_retry_after(mut self, seconds: u32) -> Self {
        self.retry_after_seconds = Some(seconds);
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
        if self.status == StatusCode::SERVICE_UNAVAILABLE {
            let retry_after = self.retry_after_seconds.unwrap_or(5).to_string();
            if let Ok(value) = HeaderValue::from_str(&retry_after) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store, max-age=0"),
            );
            response.headers_mut().insert(
                HeaderName::from_static("x-robots-tag"),
                HeaderValue::from_static("noindex, nofollow, noarchive"),
            );
        }
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

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;

    use super::*;

    #[test]
    fn service_unavailable_is_not_cacheable_or_indexable_and_is_retryable() {
        let response = ApiError::service_unavailable("database is restarting").into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "5");
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store, max-age=0"
        );
        assert_eq!(
            response.headers().get("x-robots-tag").unwrap(),
            "noindex, nofollow, noarchive"
        );
    }

    #[tokio::test]
    async fn stable_domain_problem_codes_have_canonical_type_uris() {
        for code in STABLE_DOMAIN_PROBLEM_CODES {
            let response = ApiError::conflict("contract test")
                .with_code(code)
                .into_response();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(problem["type"], problem_type_uri(code));
        }
    }
}
