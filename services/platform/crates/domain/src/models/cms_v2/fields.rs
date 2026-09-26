use super::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeoInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub indexable: bool,
    pub social_image: Option<MediaUseReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaxonomyTypeFields {
    pub key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorialTypeFields {
    pub category: Option<String>,
    pub author_display_name: Option<String>,
    pub publication_at: Option<DateTime<Utc>>,
    pub cover: Option<MediaUseReference>,
    pub featured: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FaqItem {
    pub id: Uuid,
    pub question: String,
    pub answer: TiptapDocument,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FaqTypeFields {
    pub items: Vec<FaqItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaseStudyTypeFields {
    pub industry: Option<String>,
    pub location: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadTypeFields {
    pub version_label: Option<String>,
    pub resource_type: Option<String>,
    pub version_notes: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegalTypeFields {
    pub effective_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContactInformationInput {
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address_lines: Vec<String>,
    pub locality: Option<String>,
    pub region: Option<String>,
    pub postal_code: Option<String>,
    pub country_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SocialLinkInput {
    pub service: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductCategoryPresentationInput {
    pub code: ProductFamily,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneralInformationTypeFields {
    pub organization_name: Option<String>,
    pub brand_line: Option<String>,
    pub site_icon: Option<AssetVersionReference>,
    pub home_path: Option<String>,
    pub footer_statement: Option<String>,
    pub copyright_template: Option<String>,
    pub contact: ContactInformationInput,
    pub social_links: Vec<SocialLinkInput>,
    pub default_seo: SeoInput,
    pub product_categories: Vec<ProductCategoryPresentationInput>,
    pub navigation_cta: Option<EditorialAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationItem {
    pub id: Uuid,
    pub label: String,
    pub target: Option<LinkTargetReference>,
    pub children: Vec<NavigationItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationTypeFields {
    pub items: Vec<NavigationItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FooterColumn {
    pub id: Uuid,
    pub title: String,
    pub links: Vec<NavigationItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FooterTypeFields {
    pub columns: Vec<FooterColumn>,
    pub legal_links: Vec<NavigationItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ContentTypeFields {
    Home,
    Page,
    Solution(TaxonomyTypeFields),
    Technology(TaxonomyTypeFields),
    Article(EditorialTypeFields),
    News(EditorialTypeFields),
    Faq(FaqTypeFields),
    CaseStudy(CaseStudyTypeFields),
    Download(DownloadTypeFields),
    Company,
    Legal(LegalTypeFields),
    GeneralInformation(Box<GeneralInformationTypeFields>),
    Navigation(NavigationTypeFields),
    Footer(FooterTypeFields),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentDraftV2 {
    pub schema_version: u16,
    pub kind: CmsContentKind,
    pub locale: String,
    pub template_key: ContentTemplateKey,
    pub title: String,
    pub slug: Option<String>,
    pub summary: Option<String>,
    pub is_placeholder: bool,
    pub type_fields: ContentTypeFields,
    pub body: Option<TiptapDocument>,
    pub composition: PageComposition,
    pub seo: SeoInput,
    pub relations: Vec<ContentRelationReference>,
    pub draft_version: i64,
}
