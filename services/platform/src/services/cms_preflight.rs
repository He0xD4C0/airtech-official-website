mod analyze;
mod composition;
mod composition_validation;
mod conversion;
mod counts;
mod document;
mod download;
mod general_information;
mod general_information_contact;
mod general_information_fields;
mod general_information_records;
mod integrity;
mod load;
mod news;
mod ordering;
mod references;
mod routes;
mod singletons;
mod support;
mod templates;
mod type_fields;
mod types;

pub use analyze::{analyze_legacy_snapshot, plan_legacy_snapshot, LegacyMigrationPlan};
pub use load::{load_legacy_snapshot, load_legacy_snapshot_from_connection};
pub use types::{
    CmsPreflightRecord, CmsPreflightRecordRole, LegacyAssetReference, LegacyContentEntry,
    LegacyContentRelation, LegacyContentRevision, LegacyGeneralInformation,
    LegacyGeneralInformationRevision, LegacyMediaAsset, LegacyNews, LegacyNewsWorking,
    LegacyProductIdentity, LegacyPublicRoute, LegacySnapshot,
};

use chrono::Utc;
use sqlx::PgPool;

use crate::models::MigrationPreflightReport;

/// Runs a read-only, repeatable-read migration preflight and returns the public
/// CMS V2 report contract. This function never writes migration candidates.
pub async fn run_preflight(pool: &PgPool) -> Result<MigrationPreflightReport, sqlx::Error> {
    let snapshot = load_legacy_snapshot(pool).await?;
    Ok(analyze_legacy_snapshot(snapshot, Utc::now()))
}

/// CLI-oriented alias kept intentionally small so callers share one report path.
pub async fn run(pool: &PgPool) -> Result<MigrationPreflightReport, sqlx::Error> {
    run_preflight(pool).await
}

#[cfg(test)]
mod download_tests;
#[cfg(test)]
mod integrity_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod route_tests;

#[cfg(test)]
mod singleton_tests;
