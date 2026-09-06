#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContentRevisionKindV2 {
    Manual,
    Publish,
    Restore,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContentSnapshotIntent {
    Manual,
    Publish,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateContentSnapshotRequest {
    pub intent: ContentSnapshotIntent,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreContentRevisionRequest {
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentRecordV2 {
    pub id: Uuid,
    pub status: CmsPublicationStatusV2,
    pub draft: ContentDraftV2,
    pub latest_revision: Option<i64>,
    pub published_revision: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentRevisionV2 {
    pub content_id: Uuid,
    pub revision: i64,
    pub source_draft_version: i64,
    pub kind: ContentRevisionKindV2,
    pub document: ContentDraftV2,
    pub reason: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentDiffChange {
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentDiffV2 {
    pub content_id: Uuid,
    pub base_revision: i64,
    pub target_revision: Option<i64>,
    pub target_draft_version: Option<i64>,
    pub changes: Vec<ContentDiffChange>,
}
