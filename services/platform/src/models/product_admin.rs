use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductImportRequest {
    pub csv: String,
    pub mapping_version: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductImportError {
    pub row_number: i32,
    pub stable_id: Option<String>,
    pub field_name: Option<String>,
    pub severity: String,
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingAssetReference {
    pub stable_id: String,
    pub asset_type: String,
    pub source_reference: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductImportResult {
    pub id: Uuid,
    pub checksum: String,
    pub mapping_version: String,
    pub status: String,
    pub total_rows: i64,
    pub valid_rows: i64,
    pub malformed_rows: i64,
    pub errors: Vec<ProductImportError>,
    pub missing_assets: Vec<MissingAssetReference>,
    pub reused: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductPresentation {
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub seo: SeoMetadata,
    pub indexable: bool,
    pub sort_order: i32,
    pub related_content_ids: Vec<Uuid>,
    /// Independent portal-owned presentation revision used by ETag/If-Match.
    /// This is deliberately unrelated to `Product::current_revision`.
    pub revision: i64,
    pub published_revision: Option<i64>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProductPresentation {
    pub locale: String,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    pub seo: SeoMetadata,
    pub indexable: bool,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub related_content_ids: Vec<Uuid>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminProductDetail {
    #[serde(flatten)]
    pub product: Product,
    pub source_kind: DataClass,
    pub missing_assets: Vec<MissingAssetReference>,
    pub presentation: Option<ProductPresentation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProductPrivatePricing {
    pub product_id: Uuid,
    pub stable_id: String,
    pub source_row_number: i32,
    /// Original Product Master pricing column names and raw values. No currency
    /// or numeric interpretation is inferred by the platform.
    pub pricing_fields: BTreeMap<String, String>,
}
