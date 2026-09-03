#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RfqJourney {
    Product,
    Selection,
    Project,
    Replacement,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessContact {
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub company: Option<String>,
    pub country_or_region: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductContext {
    pub product_id: Uuid,
    pub stable_id: String,
    pub model: Option<String>,
    pub published_revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRfqRequest {
    pub journey: RfqJourney,
    pub contact: BusinessContact,
    pub product_context: Option<ProductContext>,
    pub source_path: String,
    pub locale: String,
    pub consent: bool,
    #[serde(default)]
    pub context: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RfqSubmission {
    pub id: Uuid,
    pub reference: String,
    pub request: CreateRfqRequest,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateContactRequest {
    pub contact: BusinessContact,
    pub topic: String,
    pub message: String,
    pub source_path: String,
    pub locale: String,
    pub consent: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactRequest {
    pub id: Uuid,
    pub reference: String,
    pub request: CreateContactRequest,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}
