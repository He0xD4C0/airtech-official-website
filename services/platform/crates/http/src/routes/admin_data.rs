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
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::routes::{actor, etag, parse_if_match};
use airtek_domain::models::{
    AdminProductDetail, AdminRoleRecord, AdminUserRecord, AuditEvent, CreateAdminRole, CursorPage,
    GuestSourceDaily, InviteAdminUser, ProductImportRequest, ProductImportResult, UpdateAdminRole,
    UpdateAdminUser, UpdateProductPresentation, UserInvitation,
};
use airtek_runtime::auth::AdminPrincipal;
use airtek_runtime::error::{json_hash, ApiError};
use airtek_runtime::idempotency::{
    begin as begin_idempotency, idempotency_key as parse_idempotency_key, IdempotencyOutcome,
};
use airtek_runtime::pagination::CursorQuery;
use airtek_runtime::services::invitation_replay::{
    open_invitation_replay, seal_invitation_replay, EncryptedInvitationReplay,
};
use airtek_runtime::services::{
    admin_products::{load_admin_product_detail, overlay_product_presentation},
    identity::{
        load_admin_role, load_admin_role_in_transaction, load_admin_user,
        load_admin_user_in_transaction, validate_roles,
    },
    product_import::{
        load_product_import_result as load_stored_product_import, parse_product_master,
        stage_and_queue_product_import,
    },
};
use airtek_runtime::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/products/imports",
            get(list_product_imports)
                .post(import_products)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024 + 64 * 1024)),
        )
        .route("/products/imports/{id}", get(get_product_import))
        .route("/source-metadata", get(list_source_metadata))
        .route("/products/{id}", get(get_admin_product))
        .route("/products/{id}/private-pricing", get(get_private_pricing))
        .route(
            "/products/{id}/presentation",
            patch(update_product_presentation),
        )
        .route("/analytics/sources", get(list_guest_sources))
        .route("/users", get(list_users))
        .route("/users/{id}", get(get_user).patch(update_user))
        .route("/users/{id}/sessions", delete(revoke_user_sessions))
        .route("/user-invitations", get(list_invitations).post(invite_user))
        .route("/user-invitations/{id}/revoke", post(revoke_invitation))
        .route("/roles", get(list_roles).post(create_role))
        .route(
            "/roles/{id}",
            get(get_role).patch(update_role).delete(delete_role),
        )
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReasonRequest {
    reason: String,
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

#[path = "admin_data/product_imports.rs"]
mod product_imports;
use product_imports::*;
#[path = "admin_data/source_metadata.rs"]
mod source_metadata;
use source_metadata::*;
#[path = "admin_data/product_presentation.rs"]
mod product_presentation;
use product_presentation::*;
#[path = "admin_data/analytics.rs"]
mod analytics;
use analytics::*;
#[path = "admin_data/users.rs"]
mod users;
use users::*;
#[path = "admin_data/roles.rs"]
mod roles;
use roles::*;
#[path = "admin_data/invitations.rs"]
mod invitations;
use invitations::*;
#[path = "admin_data/audit.rs"]
mod audit;
use audit::*;
#[path = "admin_data/product_validation.rs"]
mod product_validation;
use product_validation::*;
#[path = "admin_data/invitation_replay.rs"]
mod invitation_replay;
use invitation_replay::*;
#[path = "admin_data/response_helpers.rs"]
mod response_helpers;
use response_helpers::*;
#[cfg(test)]
#[path = "admin_data/tests.rs"]
mod tests;
