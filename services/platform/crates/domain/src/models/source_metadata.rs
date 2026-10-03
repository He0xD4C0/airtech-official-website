use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SourceMetadataKind {
    Supplier,
    Brand,
}

impl SourceMetadataKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Supplier => "supplier",
            Self::Brand => "brand",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "supplier" => Some(Self::Supplier),
            "brand" => Some(Self::Brand),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProductSourceMetadata {
    pub id: Uuid,
    pub kind: SourceMetadataKind,
    pub label: String,
    pub source_table: String,
    pub source_record_id: String,
    pub archive_sha256: String,
    pub attributes: Value,
    pub raw_fields: Value,
    pub captured_at: DateTime<Utc>,
    pub imported_at: DateTime<Utc>,
}
