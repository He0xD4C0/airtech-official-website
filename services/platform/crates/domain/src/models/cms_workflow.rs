use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CmsDraftState {
    Editing,
    PendingReview,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsPrivateDraft {
    pub draft_id: Uuid,
    pub content_id: Uuid,
    pub owner_user_id: Option<Uuid>,
    pub document: ContentDraftV2,
    pub draft_version: i64,
    pub base_publication_version: i64,
    pub state: CmsDraftState,
    pub rejection_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsPublishedContent {
    pub content_id: Uuid,
    pub document: ContentDraftV2,
    pub publication_version: i64,
    pub published_by: Option<Uuid>,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsReviewItem {
    pub draft: CmsPrivateDraft,
    pub submitted_by_user_id: Uuid,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsDraftPage {
    pub items: Vec<CmsPrivateDraft>,
    pub next_cursor: Option<String>,
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsPublishedPage {
    pub items: Vec<CmsPublishedContent>,
    pub next_cursor: Option<String>,
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsReviewPage {
    pub items: Vec<CmsReviewItem>,
    pub next_cursor: Option<String>,
    pub total: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsSiteSingletonState {
    pub own_draft: Option<CmsPrivateDraft>,
    pub published: Option<CmsPublishedContent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsDraftSharesRequest {
    pub user_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsRejectRequest {
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsPublishResult {
    pub content_id: Uuid,
    pub publication_version: i64,
    pub published_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CmsSubmitResult {
    pub status: String,
    pub draft: Option<CmsPrivateDraft>,
    pub publication: Option<CmsPublishResult>,
}
