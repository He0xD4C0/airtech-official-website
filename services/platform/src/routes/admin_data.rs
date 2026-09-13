use std::collections::{BTreeMap, HashSet};

use axum::{
    extract::{DefaultBodyLimit, Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    auth::AdminPrincipal,
    config::InvitationReplayEncryptionKey,
    error::{json_hash, ApiError},
    idempotency::{
        begin as begin_idempotency, idempotency_key as parse_idempotency_key, IdempotencyOutcome,
    },
    models::{
        AdminProductDetail, AdminRoleRecord, AdminUserRecord, AuditEvent, CursorPage, DataClass,
        GuestSourceDaily, GuestVisitAggregate, InviteAdminUser, ProductImportRequest,
        ProductImportResult, ProductPresentation, ProductPrivatePricing, UpdateAdminRole,
        UpdateAdminUser, UpdateProductPresentation, UserInvitation,
    },
    pagination::{
        cursor_limit, decode_scoped_cursor, encode_scoped_cursor, paginate_by_id, CursorQuery,
    },
    routes::{actor, etag, parse_if_match},
    services::product_import::{
        load_product_import_result as load_stored_product_import, parse_product_master,
        stage_and_queue_product_import,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/products/imports",
            get(list_product_imports)
                .post(import_products)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024 + 64 * 1024)),
        )
        .route("/products/imports/{id}", get(get_product_import))
        .route("/products/{id}", get(get_admin_product))
        .route("/products/{id}/private-pricing", get(get_private_pricing))
        .route(
            "/products/{id}/presentation",
            patch(update_product_presentation),
        )
        .route("/analytics/visits", get(list_guest_visits))
        .route("/analytics/sources", get(list_guest_sources))
        .route("/users", get(list_users))
        .route("/users/{id}", get(get_user).patch(update_user))
        .route("/users/{id}/sessions", delete(revoke_user_sessions))
        .route("/user-invitations", get(list_invitations).post(invite_user))
        .route("/user-invitations/{id}/revoke", post(revoke_invitation))
        .route("/roles", get(list_roles))
        .route("/roles/{id}", get(get_role).patch(update_role))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReasonRequest {
    reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncryptedInvitationReplay {
    version: u8,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyticsQuery {
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
    status: Option<String>,
}

fn identity_query_text(value: Option<String>) -> Result<Option<String>, ApiError> {
    let value = value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if value
        .as_ref()
        .is_some_and(|value| value.chars().count() > 200)
    {
        Err(ApiError::bad_request("q must not exceed 200 characters."))
    } else {
        Ok(value)
    }
}

impl AnalyticsQuery {
    fn pagination(&self) -> CursorQuery {
        CursorQuery {
            cursor: self.cursor.clone(),
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalyticsCursor {
    bucket_date: chrono::NaiveDate,
    dimension_hash: Vec<u8>,
}

include!("admin_data/product_imports.rs");
include!("admin_data/product_presentation.rs");
include!("admin_data/analytics.rs");
include!("admin_data/users.rs");
include!("admin_data/roles.rs");
include!("admin_data/invitations.rs");
include!("admin_data/product_loading.rs");
include!("admin_data/identity_storage.rs");
include!("admin_data/analytics_memory.rs");
include!("admin_data/audit.rs");
include!("admin_data/product_validation.rs");
include!("admin_data/invitation_replay.rs");
include!("admin_data/analytics_pagination.rs");
include!("admin_data/response_helpers.rs");
include!("admin_data/tests.rs");
