use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Weak},
};

use chrono::{DateTime, Utc};
use ring::hmac;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    PgConnection, PgPool, Row,
};
#[cfg(feature = "devtools")]
use tokio::sync::RwLock;
use tokio::sync::{Mutex, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::{
    config::Config,
    error::ApiError,
    models::{
        AnalyticsConsentReceipt, AuditEvent, BackgroundOperation, ContactRequest, PlatformSettings,
        Product, RfqSubmission, TemporaryOverride, UpdatePlatformSettings, ValidationIssue,
    },
    services::product_facts::project_product_facts,
    services::product_import::verify_product_master_authority,
    services::product_publication::{
        immutable_revision_payload, issues_as_errors, validate_accepted_staging_payload,
        validate_product_master, validate_source_owned_alignment,
        validate_verified_csv_product_master, workflow_error,
    },
    services::request_metrics::RequestMetrics,
};

mod analytics;
mod core;
mod hydration;
mod idempotency_store;
mod operations;
mod product_publication;
mod product_storage;
mod product_validation;
mod product_validation_db;
mod queries;
mod settings;
mod submissions;

#[cfg(feature = "devtools")]
#[derive(Clone, Debug)]
pub(crate) struct DevtoolTokenGrant {
    pub expires_at: std::time::Instant,
    pub session_id: Uuid,
    pub actor: String,
    pub user_id: Uuid,
    pub admin_session_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct IdempotencyReplay {
    pub response: Value,
    pub response_status: u16,
}

pub(crate) struct IdempotencyGuard {
    _memory_guard: OwnedMutexGuard<()>,
    database_transaction: Option<sqlx::Transaction<'static, sqlx::Postgres>>,
    _database_slot: Option<OwnedSemaphorePermit>,
}

impl IdempotencyGuard {
    pub(crate) async fn finish(mut self) -> Result<(), ApiError> {
        if let Some(transaction) = self.database_transaction.take() {
            transaction.commit().await?;
        }
        Ok(())
    }
}

const MAX_IN_MEMORY_IDEMPOTENCY_KEYS: usize = 10_000;

fn derive_analytics_storage_id(external_id: Uuid, secret: &[u8; 32]) -> Uuid {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let digest = hmac::sign(&key, external_id.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_ref()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub auth_hash_slots: Arc<Semaphore>,
    pub request_metrics: Arc<RequestMetrics>,
    idempotency_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
    idempotency_database_slots: Arc<Semaphore>,
    #[cfg(feature = "devtools")]
    pub(crate) devtool_tokens: Arc<RwLock<HashMap<String, DevtoolTokenGrant>>>,
    #[cfg(feature = "devtools")]
    pub(crate) devtool_session_slots: Arc<Semaphore>,
}

pub(crate) fn overlay_presentation_row(product: &mut Product, row: &PgRow) -> Result<(), ApiError> {
    if row
        .try_get::<Option<i64>, _>("presentation_revision")?
        .is_none()
    {
        return Ok(());
    }
    product.locale = row.try_get("presentation_locale")?;
    product.slug = row.try_get("presentation_slug")?;
    product.title = row.try_get("presentation_title")?;
    product.summary = row.try_get("presentation_summary")?;
    product.seo = decode_payload(row.try_get("presentation_seo")?, "product presentation SEO")?;
    product.indexable = row.try_get("presentation_indexable")?;
    let content: Value = row.try_get("presentation_content")?;
    product.sort_order = content
        .get("sortOrder")
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or_default();
    product.related_content_ids = content
        .get("relatedContentIds")
        .cloned()
        .map(|value| decode_payload(value, "product related content ids"))
        .transpose()?
        .unwrap_or_default();
    Ok(())
}

fn enum_label<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) fn decode_payload<T: serde::de::DeserializeOwned>(
    value: Value,
    entity: &str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| {
        tracing::error!(%error, entity, "stored JSON payload is invalid");
        ApiError::service_unavailable(format!("Stored {entity} data is invalid."))
    })
}

fn decode_enum<T: serde::de::DeserializeOwned>(value: String, entity: &str) -> Result<T, ApiError> {
    decode_payload(Value::String(value), entity)
}

fn text_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{derive_analytics_storage_id, AppState};
    use crate::config::Config;
    use uuid::Uuid;

    #[test]
    fn analytics_session_storage_identifier_is_keyed_and_deterministic() {
        let external = Uuid::parse_str("0fe2d520-0f8d-49bb-b858-f984d46c9fea").unwrap();
        let first = derive_analytics_storage_id(external, &[0x11; 32]);
        let repeated = derive_analytics_storage_id(external, &[0x11; 32]);
        let rotated = derive_analytics_storage_id(external, &[0x22; 32]);
        assert_eq!(first, repeated);
        assert_ne!(first, external);
        assert_ne!(first, rotated);
        assert_eq!(first.get_version_num(), 8);
    }

    #[test]
    fn production_state_requires_postgresql() {
        let mut config = Config::for_test();
        config.production = true;
        config.database_url = None;
        assert!(AppState::new(config).is_err());
    }
}
