pub const CMS_V2_SCHEMA_VERSION: u16 = 2;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum CmsContentKind {
    Home,
    Page,
    Solution,
    Technology,
    Article,
    News,
    Faq,
    CaseStudy,
    Download,
    Company,
    Legal,
    GeneralInformation,
    Navigation,
    Footer,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ContentTemplateKey {
    Home,
    ProductIndex,
    ProductFamily,
    Selector,
    Compare,
    SolutionIndex,
    SolutionDetail,
    TechnologyIndex,
    TechnologyDetail,
    ArticleIndex,
    ArticleDetail,
    NewsIndex,
    NewsDetail,
    FaqIndex,
    FaqDetail,
    CaseStudyIndex,
    CaseStudyDetail,
    DownloadIndex,
    DownloadDetail,
    About,
    Contact,
    RfqRouter,
    RfqForm,
    Search,
    Legal,
    Navigation,
    Footer,
    GeneralInformation,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ContentBlockKind {
    Hero,
    Body,
    Media,
    FeatureGrid,
    Evidence,
    Cta,
    RelationCollection,
    FaqCollection,
    DownloadAsset,
    ContactBlock,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum CmsBodyPolicy {
    Required,
    Optional,
    Forbidden,
}

/// CMS V2 intentionally has no scheduled state. Scheduling is deferred until
/// the publishing model can support it without carrying the legacy workflow.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum CmsPublicationStatusV2 {
    Draft,
    Published,
    Archived,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentTemplateDefinition {
    pub key: ContentTemplateKey,
    pub content_kind: CmsContentKind,
    pub body_policy: CmsBodyPolicy,
    pub required_blocks: Vec<ContentBlockKind>,
    pub allowed_blocks: Vec<ContentBlockKind>,
    pub routable: bool,
    pub singleton_per_locale: bool,
    /// Locale-relative canonical path pattern owned by the platform registry.
    /// `{locale}` and `{slug}` are the only supported tokens.
    pub route_pattern: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TiptapRootType {
    Doc,
}

/// The root is strict so legacy `attrs.pageSlots` cannot enter V2.
/// Individual Tiptap nodes remain schema-validated application data.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TiptapDocument {
    #[serde(rename = "type")]
    pub node_type: TiptapRootType,
    pub content: Vec<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetVersionReference {
    pub asset_id: Uuid,
    pub version_id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaUseReference {
    pub asset: AssetVersionReference,
    pub alt_text: Option<String>,
    pub decorative: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "targetType",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RelationTargetReference {
    Content { content_id: Uuid },
    Product { product_id: Uuid },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentRelationReference {
    pub id: Uuid,
    pub slot: String,
    pub target: RelationTargetReference,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "targetType",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LinkTargetReference {
    Content { content_id: Uuid },
    Route { path: String },
    External { url: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorialAction {
    pub label: String,
    pub target: LinkTargetReference,
}
