use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Duration, Utc};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    config::{ApprovedProductMaster, ProductStagingEncryptionKey},
    error::ApiError,
    models::{MissingAssetReference, ProductImportError, ProductImportResult},
    services::product_facts::project_product_facts,
};

const MAX_CSV_BYTES: usize = 16 * 1024 * 1024;
const MAX_ROWS: usize = 50_000;

#[derive(Clone, Debug)]
pub struct ImportedProductRow {
    pub row_number: i32,
    pub stable_id: String,
    pub normalized_payload: Value,
    /// Nonce-prefixed AES-256-GCM ciphertext. This value is deliberately not
    /// serializable and is persisted only in the private staging table.
    pub confidential_payload: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct ParsedProductImport {
    pub result: ProductImportResult,
    pub rows: Vec<ImportedProductRow>,
}

#[derive(Clone, Copy, Debug)]
pub struct PrivatePricingEnvelope<'a> {
    pub mapping_version: &'a str,
    pub checksum: &'a str,
    pub source_row_number: i32,
    pub stable_id: &'a str,
    pub nonce: &'a [u8],
    pub ciphertext: &'a [u8],
    pub authentication_tag: &'a [u8],
}

/// Result of durably accepting a Product Master import. The operation id is
/// intentionally the same UUID as the import run and background job so a
/// retry cannot create detached jobs or ambiguous progress streams.
#[derive(Clone, Debug)]
pub struct StagedProductImport {
    pub operation_id: Uuid,
    pub result: ProductImportResult,
    pub queued: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductMasterAuthorityDecision {
    ProductionApproved,
    ConfiguredApprovedDevelopment,
    NonProductionValidation,
}

impl ProductMasterAuthorityDecision {
    fn audit_label(self) -> &'static str {
        match self {
            Self::ProductionApproved => "productionApprovedSource",
            Self::ConfiguredApprovedDevelopment => "configuredApprovedDevelopmentSource",
            Self::NonProductionValidation => "nonProductionValidation",
        }
    }
}

/// Apply the exact source decision before staging, again in the worker, and
/// again before publication. Production never infers authority from a
/// successfully parsed shape or from an earlier database status alone.
pub fn verify_product_master_authority(
    environment: &str,
    approved: Option<&ApprovedProductMaster>,
    checksum: &str,
    mapping_version: &str,
    valid_rows: i64,
    error_rows: i64,
) -> Result<ProductMasterAuthorityDecision, ApiError> {
    if !matches!(environment, "production" | "development" | "test") {
        return Err(ApiError::bad_request(
            "Product import environment must be production, development, or test.",
        ));
    }
    let matches_approved = approved.is_some_and(|authority| {
        authority.sha256.eq_ignore_ascii_case(checksum)
            && authority.mapping_version == mapping_version
            && authority.expected_valid_rows == valid_rows
            && authority.expected_error_rows == error_rows
    });
    if environment == "production" {
        if approved.is_none() {
            return Err(ApiError::service_unavailable(
                "Production Product Master authority is not configured.",
            ));
        }
        if !matches_approved {
            return Err(ApiError::conflict(
                "Product Master source, mapping, or reviewed row counts do not match the production authority registration.",
            ));
        }
        return Ok(ProductMasterAuthorityDecision::ProductionApproved);
    }
    if approved.is_some() && !matches_approved {
        return Err(ApiError::conflict(
            "Product Master source, mapping, or reviewed row counts do not match the configured authority registration.",
        ));
    }
    Ok(if matches_approved {
        ProductMasterAuthorityDecision::ConfiguredApprovedDevelopment
    } else {
        ProductMasterAuthorityDecision::NonProductionValidation
    })
}

#[path = "product_import/staging.rs"]
mod staging;
#[cfg(test)]
use staging::product_import_job_payload;
pub use staging::stage_and_queue_product_import;
#[path = "product_import/promotion.rs"]
mod promotion;
pub use promotion::*;
#[path = "product_import/loading_and_parsing.rs"]
mod loading_and_parsing;
use loading_and_parsing::*;
pub use loading_and_parsing::*;
#[path = "product_import/csv_and_crypto.rs"]
mod csv_and_crypto;
pub use csv_and_crypto::decrypt_private_pricing;
pub(crate) use csv_and_crypto::encrypt_confidential;
use csv_and_crypto::parse_csv;
#[path = "product_import/normalization.rs"]
mod normalization;
use normalization::*;
#[path = "product_import/specifications.rs"]
mod specifications;
use specifications::*;
#[path = "product_import/operating_conditions_and_assets.rs"]
mod operating_conditions_and_assets;
use operating_conditions_and_assets::*;
#[cfg(test)]
#[path = "product_import/tests.rs"]
mod tests;
