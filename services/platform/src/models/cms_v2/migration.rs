#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MigrationPreflightSeverity {
    Warning,
    Blocking,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MigrationPreflightSource {
    Content,
    ContentRevision,
    News,
    GeneralInformation,
    Media,
    Relation,
    Route,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MigrationPreflightIssueCode {
    InvalidLegacyPayload,
    UnsupportedContentKind,
    UnsupportedTemplate,
    EmbeddedPageSlots,
    UnknownBlock,
    InvalidTiptapDocument,
    TypeFieldMismatch,
    InvalidRelation,
    RelationHistoryUnavailable,
    MissingRelationTarget,
    MissingMediaAsset,
    MissingMediaVersion,
    DuplicatePublicPath,
    MissingPublicRoute,
    RouteOwnershipMismatch,
    CanonicalPathMismatch,
    ScheduledPublicationUnsupported,
    RevisionConversionFailed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationPreflightIssue {
    pub severity: MigrationPreflightSeverity,
    pub code: MigrationPreflightIssueCode,
    pub source: MigrationPreflightSource,
    pub entity_id: Option<Uuid>,
    pub revision: Option<i64>,
    pub json_path: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationPreflightCounts {
    pub content_entries: u64,
    pub content_revisions: u64,
    pub news_entries: u64,
    pub general_information_entries: u64,
    pub media_assets: u64,
    pub media_references: u64,
    pub relations: u64,
    pub public_routes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationPreflightReport {
    pub target_schema_version: u16,
    pub generated_at: DateTime<Utc>,
    pub can_migrate: bool,
    pub scanned: MigrationPreflightCounts,
    pub convertible: MigrationPreflightCounts,
    pub blocking_issue_count: u64,
    pub warning_count: u64,
    pub issues: Vec<MigrationPreflightIssue>,
}
