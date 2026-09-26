use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HeroVariant {
    Standard,
    SplitMedia,
    Minimal,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeroBlock {
    pub id: Uuid,
    pub eyebrow: Option<String>,
    pub heading: Option<String>,
    pub lead: Option<String>,
    pub media: Option<MediaUseReference>,
    pub actions: Vec<EditorialAction>,
    pub variant: HeroVariant,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContentWidth {
    Narrow,
    Standard,
    Wide,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BodyBlock {
    pub id: Uuid,
    pub width: ContentWidth,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MediaLayout {
    Inline,
    FullWidth,
    Aside,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaBlock {
    pub id: Uuid,
    pub media: MediaUseReference,
    pub caption: Option<String>,
    pub layout: MediaLayout,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeatureItem {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<MediaUseReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeatureGridBlock {
    pub id: Uuid,
    pub heading: Option<String>,
    pub items: Vec<FeatureItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceItem {
    pub id: Uuid,
    pub label: String,
    pub statement: String,
    pub source_note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceBlock {
    pub id: Uuid,
    pub heading: Option<String>,
    pub items: Vec<EvidenceItem>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CtaVariant {
    Standard,
    Emphasized,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CtaBlock {
    pub id: Uuid,
    pub eyebrow: Option<String>,
    pub heading: String,
    pub body: Option<String>,
    pub action: EditorialAction,
    pub variant: CtaVariant,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CollectionPresentation {
    Cards,
    List,
    Compact,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RelationCollectionBlock {
    pub id: Uuid,
    pub heading: Option<String>,
    pub relation_ids: Vec<Uuid>,
    pub presentation: CollectionPresentation,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FaqCollectionBlock {
    pub id: Uuid,
    pub heading: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadAssetBlock {
    pub id: Uuid,
    pub asset: AssetVersionReference,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ContactChannelKind {
    Email,
    Phone,
    Address,
    Social,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContactBlock {
    pub id: Uuid,
    pub heading: Option<String>,
    pub channels: Vec<ContactChannelKind>,
    pub action: Option<EditorialAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ContentBlock {
    Hero(HeroBlock),
    Body(BodyBlock),
    Media(MediaBlock),
    FeatureGrid(FeatureGridBlock),
    Evidence(EvidenceBlock),
    Cta(CtaBlock),
    RelationCollection(RelationCollectionBlock),
    FaqCollection(FaqCollectionBlock),
    DownloadAsset(DownloadAssetBlock),
    ContactBlock(ContactBlock),
}

impl ContentBlock {
    pub fn kind(&self) -> ContentBlockKind {
        match self {
            Self::Hero(_) => ContentBlockKind::Hero,
            Self::Body(_) => ContentBlockKind::Body,
            Self::Media(_) => ContentBlockKind::Media,
            Self::FeatureGrid(_) => ContentBlockKind::FeatureGrid,
            Self::Evidence(_) => ContentBlockKind::Evidence,
            Self::Cta(_) => ContentBlockKind::Cta,
            Self::RelationCollection(_) => ContentBlockKind::RelationCollection,
            Self::FaqCollection(_) => ContentBlockKind::FaqCollection,
            Self::DownloadAsset(_) => ContentBlockKind::DownloadAsset,
            Self::ContactBlock(_) => ContentBlockKind::ContactBlock,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageComposition {
    pub blocks: Vec<ContentBlock>,
}
