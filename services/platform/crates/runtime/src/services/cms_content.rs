use sqlx::PgPool;

use crate::error::ApiError;
use crate::state::AppState;

mod publication;

pub use publication::{publish_current_route, resolve_content_links, resolve_relation_cards};

pub fn require_postgres(state: &AppState) -> Result<&PgPool, ApiError> {
    Ok(&state.pool)
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
