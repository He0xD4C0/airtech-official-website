use super::*;

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: Uuid,
    pub public_url: String,
    pub download_url: String,
    pub original_name: String,
    pub media_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub uploaded_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MediaAssetReference {
    pub content_id: Uuid,
    pub content_revision: i64,
    pub content_title: String,
    pub content_status: String,
    pub dependency_kind: String,
    pub reference_path: String,
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
    pub content: PublicContentProjection,
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
    pub general_information: Option<PublicContentProjection>,
    pub navigation: Option<PublicContentProjection>,
    pub footer: Option<PublicContentProjection>,
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
    pub page: Option<PublicContentProjection>,
}
