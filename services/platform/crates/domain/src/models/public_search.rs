use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum PublicSearchEntityType {
    Content,
    Product,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum PublicSearchType {
    Product,
    Solution,
    Technology,
    Article,
    News,
    Faq,
    CaseStudy,
    Download,
    Company,
    Page,
}

impl PublicSearchType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Solution => "solution",
            Self::Technology => "technology",
            Self::Article => "article",
            Self::News => "news",
            Self::Faq => "faq",
            Self::CaseStudy => "caseStudy",
            Self::Download => "download",
            Self::Company => "company",
            Self::Page => "page",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSearchItem {
    pub entity_type: PublicSearchEntityType,
    pub entity_id: Uuid,
    pub title: String,
    pub summary: Option<String>,
    pub canonical_path: String,
    pub display_type: PublicSearchType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSearchPage {
    pub items: Vec<PublicSearchItem>,
    pub next_cursor: Option<String>,
    pub total: usize,
    pub type_counts: Vec<ProductFacetCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicSearchQuery {
    pub q: Option<String>,
    #[serde(rename = "type")]
    pub content_type: Option<PublicSearchType>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}
