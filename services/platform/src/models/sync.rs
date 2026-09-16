use super::*;

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
    pub revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflictPage {
    pub items: Vec<SyncConflict>,
    pub next_cursor: Option<String>,
    pub total: usize,
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SyncConflictDecision {
    AcceptIncoming,
    KeepVerifiedLocal,
}

impl SyncConflictDecision {
    pub fn label(self) -> &'static str {
        match self {
            Self::AcceptIncoming => "acceptIncoming",
            Self::KeepVerifiedLocal => "keepVerifiedLocal",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveSyncConflictRequest {
    pub decision: SyncConflictDecision,
    pub evidence_reference: Option<String>,
    pub reason: String,
    pub expires_at: Option<DateTime<Utc>>,
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
