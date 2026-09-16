pub(super) use std::{net::SocketAddr, sync::OnceLock};

pub(super) use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
pub(super) use axum::{
    extract::{connect_info::ConnectInfo, Extension, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
pub(super) use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
pub(super) use chrono::{DateTime, Duration, Utc};
pub(super) use serde::{Deserialize, Serialize};
pub(super) use serde_json::json;
pub(super) use sha2::{Digest, Sha256};
pub(super) use sqlx::Row;
pub(super) use subtle::ConstantTimeEq;
pub(super) use uuid::Uuid;

pub(super) use crate::{
    error::ApiError,
    models::AuditEvent,
    second_factor::{generate_recovery_codes, normalize_recovery_code, EnrollmentSecret},
    state::AppState,
};
