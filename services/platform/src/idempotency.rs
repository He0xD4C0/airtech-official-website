use axum::http::{HeaderMap, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};

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

pub struct StagedIdempotency {
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

    /// Store the replay record in the caller's business transaction. The
    /// returned guard must be finished only after that transaction commits, so
    /// another request cannot pass the advisory lock before the replay row is
    /// visible.
    pub async fn stage_in_transaction<T: Serialize>(
        self,
        transaction: &mut Transaction<'_, Postgres>,
        response: &T,
        status: StatusCode,
    ) -> Result<StagedIdempotency, ApiError> {
        let response = serde_json::to_value(response)
            .map_err(|_| ApiError::internal("Idempotency response serialization failed."))?;
        let key_hash = format!("{:x}", Sha256::digest(self.key.as_bytes()));
        let result = sqlx::query(
            r#"INSERT INTO idempotency_keys
               (scope,key_hash,request_hash,response_status,response_body)
               VALUES ($1,$2,$3,$4,$5)
               ON CONFLICT (scope,key_hash) DO UPDATE SET
                 request_hash=EXCLUDED.request_hash,
                 response_status=EXCLUDED.response_status,
                 response_body=EXCLUDED.response_body,
                 created_at=now(),expires_at=now() + interval '24 hours'
               WHERE idempotency_keys.expires_at <= now()"#,
        )
        .bind(self.scope)
        .bind(key_hash)
        .bind(self.request_hash)
        .bind(i32::from(status.as_u16()))
        .bind(response)
        .execute(&mut **transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "This Idempotency-Key is already active for another request.",
            ));
        }
        Ok(StagedIdempotency { guard: self.guard })
    }
}

impl StagedIdempotency {
    pub async fn finish(self) -> Result<(), ApiError> {
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
