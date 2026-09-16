use super::*;

/// The small, deliberately allow-listed set of business policy values that can
/// be changed from the Admin application. Deployment topology, credentials and
/// analytics/provider configuration are intentionally not part of this model.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSettings {
    pub rfq_retention_days: i64,
    pub retention_deletion_grace_days: i64,
    pub temporary_override_default_days: i64,
    pub public_locale: String,
    pub revision: i64,
}

impl Default for PlatformSettings {
    fn default() -> Self {
        Self {
            rfq_retention_days: 365,
            retention_deletion_grace_days: 30,
            temporary_override_default_days: 30,
            public_locale: "en".into(),
            revision: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePlatformSettings {
    pub rfq_retention_days: Option<i64>,
    pub retention_deletion_grace_days: Option<i64>,
    pub temporary_override_default_days: Option<i64>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub status: String,
    pub service: String,
    pub version: String,
    pub persistence: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedResponse {
    pub id: Uuid,
    pub reference: String,
    pub accepted_at: DateTime<Utc>,
}

pub(super) fn default_locale() -> String {
    "en".into()
}

pub(super) fn editorial_data_class() -> DataClass {
    DataClass::Editorial
}
