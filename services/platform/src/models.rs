use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> CursorPage<T> {
    pub fn all(items: Vec<T>) -> Self {
        Self {
            items,
            next_cursor: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    Home,
    Solution,
    Technology,
    Article,
    News,
    Faq,
    CaseStudy,
    Download,
    Company,
    Legal,
    Navigation,
    Footer,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DataClass {
    Editorial,
    Feishu,
    VerifiedCsv,
    DevelopmentFixture,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PublicationStatus {
    Draft,
    Scheduled,
    Published,
    Archived,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RichTextDocument {
    pub schema_version: u16,
    pub doc: Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SeoMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub canonical_path: Option<String>,
    pub indexable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContentEntry {
    pub id: Uuid,
    pub kind: ContentKind,
    pub slug: String,
    pub locale: String,
    pub title: String,
    pub summary: Option<String>,
    pub body: RichTextDocument,
    pub seo: SeoMetadata,
    pub status: PublicationStatus,
    pub is_placeholder: bool,
    pub current_revision: i64,
    pub published_revision: Option<i64>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentDraftInput {
    pub kind: ContentKind,
    pub slug: String,
    #[serde(default = "default_locale")]
    pub locale: String,
    pub title: String,
    pub summary: Option<String>,
    pub body: RichTextDocument,
    #[serde(default)]
    pub seo: SeoMetadata,
    #[serde(default)]
    pub is_placeholder: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewsDraftInput {
    pub content: ContentDraftInput,
    pub category: String,
    pub author_display_name: String,
    pub cover_media_id: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub featured: bool,
    #[serde(default = "editorial_data_class")]
    pub data_class: DataClass,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsEntry {
    pub content: ContentEntry,
    pub category: String,
    pub author_display_name: Option<String>,
    pub cover_media_id: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
    pub featured: bool,
    pub data_class: DataClass,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneralInformationDraftInput {
    #[serde(default = "default_locale")]
    pub locale: String,
    pub payload: Value,
    #[serde(default)]
    pub is_placeholder: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneralInformation {
    pub id: Uuid,
    pub locale: String,
    pub payload: Value,
    pub status: PublicationStatus,
    pub current_revision: i64,
    pub published_revision: Option<i64>,
    pub is_placeholder: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductFamilyPresentation {
    pub code: ProductFamily,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteBootstrap {
    pub general_information: Option<GeneralInformation>,
    pub navigation: Option<ContentEntry>,
    pub footer: Option<ContentEntry>,
    pub product_families: Vec<ProductFamilyPresentation>,
    pub motor_technologies: Vec<String>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteResolution {
    pub path: String,
    pub template_key: String,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub locale: String,
    pub published_revision: Option<i64>,
    pub indexable: bool,
    pub data_class: DataClass,
    pub page: Option<ContentEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateContentPreviewRequest {
    pub revision: i64,
    pub expires_in_seconds: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPreviewLink {
    pub url: String,
    pub content_id: Uuid,
    pub revision: i64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentPreviewResponse {
    pub content: ContentEntry,
    pub preview_expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ProductFamily {
    Centrifugal,
    Axial,
    CrossFlow,
    InlineDuct,
    Motors,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FactState {
    Verified,
    Missing,
    NotApplicable,
    NotTested,
    Confidential,
    PendingVerification,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpecValue {
    pub key: String,
    pub label: String,
    pub value: Option<Value>,
    pub unit: Option<String>,
    pub operating_condition: Option<String>,
    pub state: FactState,
    pub source_reference: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurvePoint {
    pub airflow: f64,
    pub pressure: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceCurve {
    pub airflow_unit: String,
    pub pressure_unit: String,
    pub speed_rpm: Option<u32>,
    pub density_kg_m3: Option<f64>,
    pub voltage: Option<String>,
    pub test_method: Option<String>,
    pub source_reference: String,
    pub state: FactState,
    pub points: Vec<CurvePoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: Uuid,
    pub stable_id: String,
    pub model: Option<String>,
    pub slug: String,
    pub locale: String,
    pub family: ProductFamily,
    pub subtype: Option<String>,
    pub motor_technology: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub seo: SeoMetadata,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub related_content_ids: Vec<Uuid>,
    pub specifications: Vec<SpecValue>,
    pub performance_curves: Vec<PerformanceCurve>,
    pub source_snapshot_id: Uuid,
    pub source_revision: String,
    pub current_revision: i64,
    pub published_revision: Option<i64>,
    pub status: PublicationStatus,
    pub indexable: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductQuery {
    pub family: Option<ProductFamily>,
    pub motor_technology: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorRequest {
    pub airflow: f64,
    pub airflow_unit: String,
    pub pressure: f64,
    pub pressure_unit: String,
    pub ambient_temperature_c: Option<f64>,
    pub maximum_diameter_mm: Option<f64>,
    pub voltage: Option<String>,
    pub frequency_hz: Option<f64>,
    pub required_certifications: Vec<String>,
    pub preferred_family: Option<ProductFamily>,
    pub motor_technology: Option<String>,
    pub priority: Option<SelectorPriority>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SelectorPriority {
    Efficiency,
    Noise,
    Size,
    Headroom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorCandidate {
    pub product_id: Uuid,
    pub product_revision: i64,
    pub title: String,
    pub matched_constraints: Vec<String>,
    pub warnings: Vec<String>,
    pub rank: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorResponse {
    pub outcome: SelectorOutcome,
    pub candidates: Vec<SelectorCandidate>,
    pub explanations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SelectorOutcome {
    Matched,
    NoValidatedCandidates,
    EngineeringReviewRequired,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncRunStatus {
    Queued,
    Fetching,
    Validating,
    AwaitingResolution,
    ReadyToPublish,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSyncRequest {
    #[serde(default)]
    pub dry_run: bool,
    pub mapping_version: String,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRun {
    pub id: Uuid,
    pub source: String,
    pub dry_run: bool,
    pub mapping_version: String,
    pub status: SyncRunStatus,
    pub resume_cursor: Option<String>,
    pub records_seen: u64,
    pub records_valid: u64,
    pub conflict_count: u64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSnapshot {
    pub id: Uuid,
    pub connector_id: Uuid,
    pub sync_run_id: Uuid,
    pub source_record_id: String,
    pub source_revision: String,
    pub checksum: String,
    pub source_payload: Value,
    pub received_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StagingValidationStatus {
    Pending,
    Valid,
    Invalid,
    Conflicted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    pub field_path: String,
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagingRecord {
    pub id: Uuid,
    pub sync_run_id: Uuid,
    pub source_snapshot_id: Uuid,
    pub source_record_id: String,
    pub validation_status: StagingValidationStatus,
    pub normalized_payload: Option<Value>,
    pub validation_errors: Vec<ValidationIssue>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDiff {
    pub field_path: String,
    pub base_value: Option<Value>,
    pub local_value: Option<Value>,
    pub incoming_value: Option<Value>,
    pub source_owned: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub id: Uuid,
    pub sync_run_id: Uuid,
    pub product_id: Option<Uuid>,
    pub source_record_id: String,
    pub diffs: Vec<FieldDiff>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTemporaryOverride {
    pub product_id: Uuid,
    pub field_path: String,
    pub value: Value,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporaryOverride {
    pub id: Uuid,
    pub product_id: Uuid,
    pub field_path: String,
    pub value: Value,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub expired: bool,
}

impl TemporaryOverride {
    pub fn from_input(input: CreateTemporaryOverride) -> Self {
        let created_at = Utc::now();
        let expires_at = input
            .expires_at
            .unwrap_or_else(|| created_at + Duration::days(30));
        Self {
            id: Uuid::new_v4(),
            product_id: input.product_id,
            field_path: input.field_path,
            value: input.value,
            reason: input.reason,
            created_at,
            expires_at,
            expired: expires_at <= created_at,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RfqJourney {
    Product,
    Selection,
    Project,
    Replacement,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessContact {
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub company: Option<String>,
    pub country_or_region: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductContext {
    pub product_id: Uuid,
    pub stable_id: String,
    pub model: Option<String>,
    pub published_revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRfqRequest {
    pub journey: RfqJourney,
    pub contact: BusinessContact,
    pub product_context: Option<ProductContext>,
    pub source_path: String,
    pub locale: String,
    pub consent: bool,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RfqSubmission {
    pub id: Uuid,
    pub reference: String,
    pub request: CreateRfqRequest,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateContactRequest {
    pub contact: BusinessContact,
    pub topic: String,
    pub message: String,
    pub source_path: String,
    pub locale: String,
    pub consent: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactRequest {
    pub id: Uuid,
    pub reference: String,
    pub request: CreateContactRequest,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnalyticsConsent {
    pub anonymous_session_id: Uuid,
    pub policy_version: String,
    pub analytics_allowed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsConsentReceipt {
    pub consent_receipt: Uuid,
    pub anonymous_session_id: Uuid,
    pub policy_version: String,
    pub analytics_allowed: bool,
    pub granted_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnalyticsEvent {
    pub event_name: String,
    pub anonymous_session_id: Option<Uuid>,
    pub source_path: String,
    pub locale: String,
    pub consent_granted: bool,
    pub policy_version: Option<String>,
    pub consent_receipt: Option<Uuid>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsEventReceipt {
    pub accepted: bool,
    pub event_id: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct StoredAnalyticsEvent {
    pub event_name: String,
    pub guest_visit_id: Uuid,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsOverviewRange {
    pub from: DateTime<Utc>,
    pub to_exclusive: DateTime<Utc>,
    pub timezone: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsConsentedMetrics {
    pub visits: i64,
    pub page_views: i64,
    pub engaged_visit_days: i64,
    pub rfq_start_events: i64,
    pub rfq_submit_events: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsBusinessOutcomes {
    pub rfq_submissions: i64,
    pub contact_requests: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsOverview {
    pub range: AnalyticsOverviewRange,
    pub generated_at: DateTime<Utc>,
    pub consented_metrics: AnalyticsConsentedMetrics,
    pub business_outcomes: AnalyticsBusinessOutcomes,
    pub source: String,
    pub contains_pii: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateGuestVisit {
    pub anonymous_session_id: Uuid,
    pub consent_receipt: Uuid,
    pub policy_version: String,
    pub landing_path: String,
    pub referrer_domain: Option<String>,
    pub source: Option<String>,
    pub medium: Option<String>,
    pub campaign: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestVisit {
    pub id: Uuid,
    pub anonymous_session_id: Uuid,
    pub landing_path: String,
    pub referrer_domain: Option<String>,
    pub source: String,
    pub medium: Option<String>,
    pub campaign: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestSourceDaily {
    pub bucket_date: chrono::NaiveDate,
    pub source: String,
    pub source_name: Option<String>,
    pub referrer_domain: Option<String>,
    pub utm_source: Option<String>,
    pub medium: Option<String>,
    pub campaign: Option<String>,
    pub landing_path: String,
    pub locale: String,
    pub visits: i64,
    pub page_views: i64,
    pub rfq_starts: i64,
    pub rfq_submissions: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestVisitAggregate {
    pub bucket_date: chrono::NaiveDate,
    pub landing_path: String,
    pub locale: String,
    pub visits: i64,
    pub page_views: i64,
    pub rfq_starts: i64,
    pub rfq_submissions: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserRecord {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub locale: String,
    pub status: String,
    pub revision: i64,
    pub roles: Vec<String>,
    pub totp_enabled: bool,
    pub invited_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminRoleRecord {
    pub id: Uuid,
    pub key: String,
    pub display_name: String,
    pub system_role: bool,
    pub revision: i64,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAdminRole {
    pub display_name: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InviteAdminUser {
    pub email: String,
    pub display_name: String,
    pub role_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInvitation {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub locale: String,
    pub role_keys: Vec<String>,
    pub status: String,
    pub invited_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Returned only from invitation creation because email delivery is not
    /// connected in phase one. Only its SHA-256 digest is stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAdminUser {
    pub display_name: Option<String>,
    pub locale: Option<String>,
    pub status: Option<String>,
    pub role_keys: Option<Vec<String>>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductImportRequest {
    pub csv: String,
    pub mapping_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductImportError {
    pub row_number: i32,
    pub stable_id: Option<String>,
    pub field_name: Option<String>,
    pub severity: String,
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingAssetReference {
    pub stable_id: String,
    pub asset_type: String,
    pub source_reference: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductImportResult {
    pub id: Uuid,
    pub checksum: String,
    pub mapping_version: String,
    pub status: String,
    pub total_rows: i64,
    pub valid_rows: i64,
    pub malformed_rows: i64,
    pub errors: Vec<ProductImportError>,
    pub missing_assets: Vec<MissingAssetReference>,
    pub reused: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductPresentation {
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub seo: SeoMetadata,
    pub indexable: bool,
    pub sort_order: i32,
    pub related_content_ids: Vec<Uuid>,
    /// Independent portal-owned presentation revision used by ETag/If-Match.
    /// This is deliberately unrelated to `Product::current_revision`.
    pub revision: i64,
    pub published_revision: Option<i64>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProductPresentation {
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub seo: SeoMetadata,
    pub indexable: bool,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub related_content_ids: Vec<Uuid>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminProductDetail {
    #[serde(flatten)]
    pub product: Product,
    pub source_kind: DataClass,
    pub missing_assets: Vec<MissingAssetReference>,
    pub presentation: Option<ProductPresentation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProductPrivatePricing {
    pub product_id: Uuid,
    pub stable_id: String,
    pub source_row_number: i32,
    /// Original Product Master pricing column names and raw values. No currency
    /// or numeric interpretation is inferred by the platform.
    pub pricing_fields: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    MigrationPreflight,
    MigrationApply,
    Backup,
    RestoreValidate,
    RetentionApply,
    SearchReindex,
    CacheInvalidate,
    FeishuSync,
    ProductImport,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOperationRequest {
    pub kind: OperationKind,
    pub reason: String,
    pub confirmation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundOperation {
    pub id: Uuid,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub result: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: Uuid,
    pub actor: String,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub reason: Option<String>,
    pub request_id: Uuid,
    pub occurred_at: DateTime<Utc>,
}

/// The small, deliberately allow-listed set of business policy values that can
/// be changed from the Admin application. Deployment topology, credentials and
/// analytics/provider configuration are intentionally not part of this model.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSettings {
    pub rfq_retention_days: i64,
    pub retention_deletion_grace_days: i64,
    pub temporary_override_default_days: i64,
    pub public_locale: String,
    pub revision: i64,
}

impl Default for PlatformSettings {
    fn default() -> Self {
        Self {
            rfq_retention_days: 365,
            retention_deletion_grace_days: 30,
            temporary_override_default_days: 30,
            public_locale: "en".into(),
            revision: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePlatformSettings {
    pub rfq_retention_days: Option<i64>,
    pub retention_deletion_grace_days: Option<i64>,
    pub temporary_override_default_days: Option<i64>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub status: String,
    pub service: String,
    pub version: String,
    pub persistence: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedResponse {
    pub id: Uuid,
    pub reference: String,
    pub accepted_at: DateTime<Utc>,
}

fn default_locale() -> String {
    "en".into()
}

fn editorial_data_class() -> DataClass {
    DataClass::Editorial
}
