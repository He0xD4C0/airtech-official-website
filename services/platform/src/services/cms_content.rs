use std::collections::BTreeMap;

use axum::http::StatusCode;
use chrono::Utc;
use serde_json::Value;
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    error::ApiError,
    idempotency::IdempotencyContext,
    models::{
        CmsPublicationStatusV2, ContentDiffV2, ContentDraftV2, ContentRecordV2,
        ContentRevisionKindV2, ContentRevisionV2, ContentSnapshotIntent,
    },
    state::AppState,
};

mod audit;
mod diff;
mod listing;
mod migration;
mod mutations;
mod publication;
mod publication_mutations;
mod storage;
mod validation;

pub use diff::diff_content;
pub use listing::{ContentListFilter, ContentListOutcome, ContentSortField, SortDirection};
pub use migration::migrate_legacy_content;
pub use mutations::{
    archive_content, create_content, restore_revision, save_draft, snapshot_content,
};
pub use publication::{publish_public_route, resolve_content_links, resolve_relation_cards};
pub use publication_mutations::unpublish_content;
pub use storage::{get_content, list_content, list_revisions};

#[derive(Clone, Debug)]
pub struct MutationMetadata {
    pub actor: String,
    pub request_id: Uuid,
}

pub fn require_postgres(state: &AppState) -> Result<&PgPool, ApiError> {
    state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("PostgreSQL is required for the unified CMS content service.")
    })
}

pub fn draft_etag(version: i64) -> String {
    format!("\"draft-{version}\"")
}

pub fn parse_draft_etag(value: &str) -> Option<i64> {
    value
        .trim_matches('"')
        .strip_prefix("draft-")
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value >= 0)
}

fn enum_label<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn decode_json<T: serde::de::DeserializeOwned>(value: Value, label: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| {
        tracing::error!(%error, label, "stored CMS JSON is invalid");
        ApiError::service_unavailable(format!("Stored {label} data is invalid."))
    })
}
