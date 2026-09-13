use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

pub(super) type CmsPreflightIssue = crate::models::MigrationPreflightIssue;
pub(super) type CmsPreflightIssueCode = crate::models::MigrationPreflightIssueCode;
pub(super) type CmsPreflightSeverity = crate::models::MigrationPreflightSeverity;
pub(super) type CmsPreflightSource = crate::models::MigrationPreflightSource;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacySnapshot {
    pub content_entries: Vec<LegacyContentEntry>,
    /// All unified content identities, loaded only as relation/link target context.
    /// Entries in this set are never converted unless they also appear above.
    pub known_content_ids: Vec<Uuid>,
    pub content_revisions: Vec<LegacyContentRevision>,
    pub news: Vec<LegacyNews>,
    pub news_working: Vec<LegacyNewsWorking>,
    pub general_information: Vec<LegacyGeneralInformation>,
    pub general_information_revisions: Vec<LegacyGeneralInformationRevision>,
    pub content_relations: Vec<LegacyContentRelation>,
    pub media_assets: Vec<LegacyMediaAsset>,
    pub asset_references: Vec<LegacyAssetReference>,
    pub public_routes: Vec<LegacyPublicRoute>,
    /// Read only to validate relation/route targets; product facts are never copied.
    pub products: Vec<LegacyProductIdentity>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyContentEntry {
    pub id: Uuid,
    pub kind: String,
    pub slug: String,
    pub locale: String,
    pub title: String,
    pub status: String,
    pub is_placeholder: bool,
    pub data_origin: String,
    pub current_revision: i64,
    pub published_revision: Option<i64>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub payload: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyContentRevision {
    pub content_id: Uuid,
    pub revision: i64,
    pub payload: Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyNews {
    pub content_id: Uuid,
    pub revision: i64,
    pub content_kind: String,
    pub category: String,
    pub author_display_name: Option<String>,
    pub cover_media_asset_id: Option<Uuid>,
    pub featured: bool,
    pub publication_at: Option<DateTime<Utc>>,
    pub reading_minutes: Option<i32>,
    pub data_origin: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyNewsWorking {
    pub content_id: Uuid,
    pub content_kind: String,
    pub category: String,
    pub author_display_name: Option<String>,
    pub cover_media_asset_id: Option<Uuid>,
    pub featured: bool,
    pub publication_at: Option<DateTime<Utc>>,
    pub reading_minutes: Option<i32>,
    pub data_origin: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyGeneralInformation {
    pub id: Uuid,
    pub scope: String,
    pub locale: String,
    pub status: String,
    pub is_placeholder: bool,
    pub data_origin: String,
    pub current_revision: i64,
    pub published_revision: Option<i64>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub payload: Value,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyGeneralInformationRevision {
    pub general_information_id: Uuid,
    pub revision: i64,
    pub locale: String,
    pub is_placeholder: bool,
    pub data_origin: String,
    pub payload: Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyContentRelation {
    pub from_type: String,
    pub from_id: Uuid,
    pub relation_type: String,
    pub to_type: String,
    pub to_id: Uuid,
    pub sort_order: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyMediaAsset {
    pub id: Uuid,
    pub storage_key: String,
    pub original_name: String,
    pub media_type: String,
    pub byte_size: i64,
    pub checksum: String,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyAssetReference {
    pub id: Uuid,
    pub media_asset_id: Uuid,
    pub content_id: Option<Uuid>,
    pub content_revision: Option<i64>,
    pub product_id: Option<Uuid>,
    pub product_revision: Option<i64>,
    pub general_information_id: Option<Uuid>,
    pub general_information_revision: Option<i64>,
    pub usage: String,
    pub locale: Option<String>,
    pub alt_text: Option<String>,
    pub sort_order: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyPublicRoute {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub locale: String,
    pub canonical_path: String,
    pub indexable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyProductIdentity {
    pub id: Uuid,
    pub locale: String,
    pub published_revision: Option<i64>,
    pub has_verified_published_localization: bool,
    pub canonical_path: Option<String>,
    pub is_placeholder: bool,
    pub indexable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CmsPreflightRecordRole {
    Working,
    Revision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmsPreflightRecord {
    pub entity_id: Uuid,
    pub source_revision: i64,
    pub role: CmsPreflightRecordRole,
    pub candidate: crate::models::ContentDraftV2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CmsV2RelationCandidate {
    pub from_id: Uuid,
    pub relation_type: String,
    pub to_type: String,
    pub to_id: Uuid,
    pub sort_order: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CmsV2MediaVersionCandidate {
    pub asset_id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CmsV2AssetReferenceCandidate {
    pub source_id: Uuid,
    pub asset_id: Uuid,
    pub owner_type: String,
    pub owner_id: Uuid,
    pub owner_revision: i64,
    pub usage: String,
    pub locale: Option<String>,
    pub alt_text: Option<String>,
    pub sort_order: i32,
}
