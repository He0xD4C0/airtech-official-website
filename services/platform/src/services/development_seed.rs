use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

const SEED_VERSION: i32 = 1;
const FIXTURE_UPDATED_AT: &str = "2026-09-02T00:00:00Z";

#[derive(Debug, Error)]
pub enum DevelopmentSeedError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("development fixture {fixture_key} is already assigned to {actual_id}, expected {expected_id}")]
    LedgerIdentityMismatch {
        fixture_key: String,
        actual_id: Uuid,
        expected_id: Uuid,
    },
    #[error("refusing to replace non-development {entity_type} row {entity_id}")]
    OwnershipMismatch {
        entity_type: &'static str,
        entity_id: Uuid,
    },
    #[error("fixture timestamp is invalid")]
    InvalidFixtureTimestamp,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentSeedReport {
    pub status: &'static str,
    pub seed_version: i32,
    pub fixture_count: usize,
    pub news_count: usize,
    pub route_count: usize,
    pub changed_fixtures: usize,
    pub unchanged_fixtures: usize,
    pub products_created: usize,
}

#[derive(Clone)]
struct ContentFixture {
    fixture_key: &'static str,
    ledger_entity_type: &'static str,
    id: Uuid,
    kind: &'static str,
    slug: &'static str,
    title: &'static str,
    summary: &'static str,
    canonical_path: Option<&'static str>,
    page_slots: Value,
    news: Option<NewsFixture>,
}

#[derive(Clone)]
struct NewsFixture {
    category: &'static str,
    publication_at: &'static str,
}

struct ExistingContent {
    current_revision: i64,
    payload: Value,
    kind: String,
    data_origin: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChangeState {
    Changed,
    Unchanged,
}

include!("development_seed/seed.rs");
include!("development_seed/content.rs");
include!("development_seed/general_information.rs");
include!("development_seed/ledger.rs");
include!("development_seed/payloads.rs");
include!("development_seed/content_fixtures.rs");
include!("development_seed/public_page_fixture.rs");
include!("development_seed/public_page_fixtures.rs");
include!("development_seed/shell_and_helpers.rs");
include!("development_seed/tests.rs");
