// Public CMS V2 projection. The platform resolves relations and content links
// server-side so the website never guesses identifiers or paths.

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ResolvedRelationEntityType {
    Content,
    Product,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedRelationCard {
    pub relation_id: Uuid,
    pub entity_type: ResolvedRelationEntityType,
    pub title: String,
    pub summary: Option<String>,
    pub href: String,
    pub eyebrow: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedLinkTarget {
    pub content_id: Uuid,
    pub href: String,
}

/// Published CMS V2 document plus the server-resolved navigation surface.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicContentProjection {
    pub schema_version: u16,
    pub id: Uuid,
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
    pub published_revision: i64,
    pub updated_at: DateTime<Utc>,
    pub resolved_relations: Vec<ResolvedRelationCard>,
    pub resolved_links: Vec<ResolvedLinkTarget>,
}
