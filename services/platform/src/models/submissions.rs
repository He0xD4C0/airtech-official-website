use super::*;

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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BusinessEntityType {
    Rfq,
    Contact,
}

impl BusinessEntityType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rfq => "rfq",
            Self::Contact => "contact",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BusinessInboxStatus {
    New,
    Triaged,
    Assigned,
    Qualified,
    Closed,
    Spam,
    PiiCleared,
}

impl BusinessInboxStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Triaged => "triaged",
            Self::Assigned => "assigned",
            Self::Qualified => "qualified",
            Self::Closed => "closed",
            Self::Spam => "spam",
            Self::PiiCleared => "piiCleared",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessInboxItem {
    pub id: Uuid,
    pub entity_type: BusinessEntityType,
    pub reference: String,
    pub journey: Option<RfqJourney>,
    pub topic: Option<String>,
    pub organization: Option<String>,
    pub country_or_region: Option<String>,
    pub product_context: Option<ProductContext>,
    pub source_path: String,
    pub locale: String,
    pub consent: bool,
    pub status: BusinessInboxStatus,
    pub revision: i64,
    pub assigned_to: Option<Uuid>,
    pub submitted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessInboxPage {
    pub items: Vec<BusinessInboxItem>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessPii {
    #[serde(default)]
    pub rfq_context: Option<RfqContextSnapshot>,
    pub name: String,
    pub email: String,
    pub phone: Option<String>,
    pub company: Option<String>,
    pub country_or_region: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessInternalNote {
    pub id: Uuid,
    pub entity_type: BusinessEntityType,
    pub entity_id: Uuid,
    pub body: String,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessStatusHistoryEntry {
    pub id: Uuid,
    pub from_status: Option<BusinessInboxStatus>,
    pub to_status: BusinessInboxStatus,
    pub reason: Option<String>,
    pub changed_by: String,
    pub changed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusinessInboxDetail {
    pub item: BusinessInboxItem,
    pub notes: Vec<BusinessInternalNote>,
    pub status_history: Vec<BusinessStatusHistoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignBusinessInboxRequest {
    pub assigned_to: Option<Uuid>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateBusinessStatusRequest {
    pub status: BusinessInboxStatus,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateBusinessNoteRequest {
    pub body: String,
    pub reason: String,
}
