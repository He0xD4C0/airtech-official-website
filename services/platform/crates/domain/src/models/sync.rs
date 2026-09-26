use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncRunStatus {
    Queued,
    Fetching,
    Validating,
    ReadyToPublish,
    Completed,
    CompletedWithErrors,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FeishuSyncTrigger {
    #[default]
    Manual,
    Interval,
    Daily,
    Initial,
}

impl FeishuSyncTrigger {
    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Interval => "interval",
            Self::Daily => "daily",
            Self::Initial => "initial",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRun {
    pub id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<Uuid>,
    pub source: String,
    pub dry_run: bool,
    #[serde(default)]
    pub trigger: FeishuSyncTrigger,
    pub settings_revision: i64,
    pub sources: Vec<FeishuSource>,
    pub mapping_version: String,
    pub status: SyncRunStatus,
    pub resume_cursor: Option<String>,
    pub records_seen: u64,
    pub records_valid: u64,
    #[serde(default)]
    pub records_applied: u64,
    #[serde(default)]
    pub records_failed: u64,
    #[serde(default)]
    pub records_deleted: u64,
    #[serde(default)]
    pub assets_seen: u64,
    #[serde(default)]
    pub assets_copied: u64,
    #[serde(default)]
    pub assets_reused: u64,
    #[serde(default)]
    pub assets_failed: u64,
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
pub struct StagingRecordPage {
    pub items: Vec<StagingRecord>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuConnectionStatus {
    pub connector_id: Option<Uuid>,
    pub display_name: Option<String>,
    pub configured: bool,
    pub enabled: bool,
    pub runnable: bool,
    pub unavailable_reason: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
    pub latest_sync: Option<SyncRun>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FeishuSource {
    pub enabled: bool,
    pub wiki_token: String,
    pub table_id: String,
    pub name: String,
    pub family: ProductFamily,
    pub application: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuSettings {
    pub connector_id: Uuid,
    pub app_id: Option<String>,
    pub secret_configured: bool,
    pub enabled: bool,
    pub interval_enabled: bool,
    pub interval_minutes: i32,
    pub daily_enabled: bool,
    pub daily_local_time: String,
    pub timezone: String,
    pub mapping_version: String,
    pub sources: Vec<FeishuSource>,
    pub revision: i64,
    pub connection_revision: i64,
    pub tested_connection_revision: Option<i64>,
    pub last_connection_test_at: Option<DateTime<Utc>>,
    pub last_interval_at: Option<DateTime<Utc>>,
    pub last_daily_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateFeishuSettings {
    pub app_id: String,
    pub app_secret: String,
    pub clear_credentials: bool,
    pub sources: Vec<FeishuSource>,
    pub enabled: bool,
    pub interval_enabled: bool,
    pub interval_minutes: i32,
    pub daily_enabled: bool,
    pub daily_local_time: String,
}

impl std::fmt::Debug for UpdateFeishuSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UpdateFeishuSettings")
            .field("app_id", &self.app_id)
            .field("app_secret", &"[redacted]")
            .field("clear_credentials", &self.clear_credentials)
            .field("sources", &self.sources)
            .field("enabled", &self.enabled)
            .field("interval_enabled", &self.interval_enabled)
            .field("interval_minutes", &self.interval_minutes)
            .field("daily_enabled", &self.daily_enabled)
            .field("daily_local_time", &self.daily_local_time)
            .finish()
    }
}

impl Drop for UpdateFeishuSettings {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.app_secret.zeroize();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuTableCheck {
    pub wiki_token: String,
    pub table_id: String,
    pub name: String,
    pub accessible: bool,
    pub field_count: usize,
    pub mapping_valid: bool,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuConnectionTest {
    pub credentials_configured: bool,
    pub token_issued: bool,
    pub object_storage_ready: bool,
    pub private_staging_ready: bool,
    pub runnable: bool,
    pub connection_revision: i64,
    pub tables: Vec<FeishuTableCheck>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuSyncError {
    pub source_record_id: Option<String>,
    pub severity: String,
    pub code: String,
    pub field_path: Option<String>,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuSyncRunDetail {
    pub run: SyncRun,
    pub tables: Vec<FeishuRunTableResult>,
    pub errors: Vec<FeishuSyncError>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuRunTableResult {
    pub sync_run_id: Uuid,
    pub wiki_token: String,
    pub table_id: String,
    pub source_name: String,
    pub status: String,
    pub records_seen: u64,
    pub records_applied: u64,
    pub records_failed: u64,
    pub records_deleted: u64,
    pub assets_seen: u64,
    pub assets_copied: u64,
    pub assets_reused: u64,
    pub assets_failed: u64,
    pub error: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncMapping {
    pub id: Uuid,
    pub connector_id: Uuid,
    pub version: String,
    pub mapping: Value,
    pub schema_version: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
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
