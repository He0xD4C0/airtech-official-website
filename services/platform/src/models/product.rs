use super::*;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ProductFamily {
    Centrifugal,
    Axial,
    CrossFlow,
    InlineDuct,
    Motors,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FactState {
    Verified,
    Missing,
    NotApplicable,
    NotTested,
    Confidential,
    PendingVerification,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpecValue {
    pub key: String,
    pub label: String,
    pub value: Option<Value>,
    pub unit: Option<String>,
    pub operating_condition: Option<String>,
    pub state: FactState,
    pub source_reference: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CurvePoint {
    pub airflow: f64,
    pub pressure: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceCurve {
    pub airflow_unit: String,
    pub pressure_unit: String,
    pub speed_rpm: Option<u32>,
    pub density_kg_m3: Option<f64>,
    pub voltage: Option<String>,
    pub test_method: Option<String>,
    pub source_reference: String,
    pub state: FactState,
    pub points: Vec<CurvePoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: Uuid,
    pub stable_id: String,
    pub model: Option<String>,
    pub slug: String,
    pub locale: String,
    pub family: ProductFamily,
    pub subtype: Option<String>,
    pub motor_technology: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub seo: SeoMetadata,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub related_content_ids: Vec<Uuid>,
    pub specifications: Vec<SpecValue>,
    pub performance_curves: Vec<PerformanceCurve>,
    pub source_snapshot_id: Uuid,
    pub source_revision: String,
    pub current_revision: i64,
    pub published_revision: Option<i64>,
    pub status: PublicationStatus,
    pub indexable: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProductPublicationAction {
    Publish,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductPublicationReport {
    pub product_id: Uuid,
    pub current_revision: i64,
    pub ready: bool,
    pub issues: Vec<ValidationIssue>,
    pub allowed_actions: Vec<ProductPublicationAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductFacetCount {
    pub value: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminProductPage {
    pub items: Vec<Product>,
    pub next_cursor: Option<String>,
    pub total: usize,
    pub family_counts: Vec<ProductFacetCount>,
    pub status_counts: Vec<ProductFacetCount>,
    pub data_state_counts: Vec<ProductFacetCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductQuery {
    pub family: Option<ProductFamily>,
    pub motor_technology: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorRequest {
    pub airflow: f64,
    pub airflow_unit: String,
    pub pressure: f64,
    pub pressure_unit: String,
    pub ambient_temperature_c: Option<f64>,
    pub maximum_diameter_mm: Option<f64>,
    pub voltage: Option<String>,
    pub frequency_hz: Option<f64>,
    pub required_certifications: Vec<String>,
    pub preferred_family: Option<ProductFamily>,
    pub motor_technology: Option<String>,
    pub priority: Option<SelectorPriority>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SelectorPriority {
    Efficiency,
    Noise,
    Size,
    Headroom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorCandidate {
    pub product_id: Uuid,
    pub product_revision: i64,
    pub title: String,
    pub matched_constraints: Vec<String>,
    pub warnings: Vec<String>,
    pub rank: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectorResponse {
    pub outcome: SelectorOutcome,
    pub candidates: Vec<SelectorCandidate>,
    pub explanations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SelectorOutcome {
    Matched,
    NoValidatedCandidates,
    EngineeringReviewRequired,
}
