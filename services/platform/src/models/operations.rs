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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventPage {
    pub items: Vec<AuditEvent>,
    pub next_cursor: Option<String>,
    pub total: usize,
}
