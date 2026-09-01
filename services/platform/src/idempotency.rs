use axum::http::{HeaderMap, StatusCode};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    error::{json_hash, ApiError},
    state::{AppState, IdempotencyGuard, IdempotencyReplay},
};

pub enum IdempotencyOutcome {
    Replay(IdempotencyReplay),
    Fresh(IdempotencyContext),
}

pub struct IdempotencyContext {
    scope: &'static str,
    key: String,
    request_hash: String,
    guard: IdempotencyGuard,
}

impl IdempotencyContext {
    pub async fn complete<T: Serialize>(
        self,
        state: &AppState,
        response: &T,
        status: StatusCode,
    ) -> Result<(), ApiError> {
        let response = serde_json::to_value(response)
            .map_err(|_| ApiError::internal("Idempotency response serialization failed."))?;
        state
            .save_idempotency(
                self.scope,
                self.key,
                self.request_hash,
                response,
                status.as_u16(),
            )
            .await?;
        self.guard.finish().await
    }
}

impl IdempotencyReplay {
    pub fn decode<T: DeserializeOwned>(&self) -> Result<T, ApiError> {
        serde_json::from_value(self.response.clone())
            .map_err(|_| ApiError::service_unavailable("Stored idempotency response is invalid."))
    }

    pub fn status(&self) -> Result<StatusCode, ApiError> {
        StatusCode::from_u16(self.response_status).map_err(|_| {
            ApiError::service_unavailable("Stored idempotency response status is invalid.")
        })
    }
}

pub async fn begin<T: Serialize>(
    state: &AppState,
    scope: &'static str,
    headers: &HeaderMap,
    request: &T,
) -> Result<IdempotencyOutcome, ApiError> {
    let key = idempotency_key(headers)?;
    let request = serde_json::to_value(request)
        .map_err(|_| ApiError::internal("Idempotency request serialization failed."))?;
    let request_hash = json_hash(&request);
    let guard = state.acquire_idempotency_guard(scope, &key).await?;
    if let Some(replay) = state.idempotency_replay(scope, &key, &request_hash).await? {
        guard.finish().await?;
        return Ok(IdempotencyOutcome::Replay(replay));
    }
    Ok(IdempotencyOutcome::Fresh(IdempotencyContext {
        scope,
        key,
        request_hash,
        guard,
    }))
}

pub fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| (8..=200).contains(&value.len()))
        .map(str::to_owned)
        .ok_or_else(|| ApiError::bad_request("Idempotency-Key must contain 8 to 200 characters."))
}
