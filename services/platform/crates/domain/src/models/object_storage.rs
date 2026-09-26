use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStorageSettings {
    pub configured: bool,
    pub provider: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub bucket: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_configured: bool,
    pub key_prefix: Option<String>,
    pub path_style: bool,
    pub public_base_url: Option<String>,
    pub legacy_asset_count: i64,
    pub revision: i64,
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
}

impl ObjectStorageSettings {
    pub fn unconfigured(legacy_asset_count: i64) -> Self {
        Self {
            configured: false,
            provider: "s3".into(),
            endpoint: None,
            region: None,
            bucket: None,
            access_key_id: None,
            secret_configured: false,
            key_prefix: Some("media".into()),
            path_style: false,
            public_base_url: None,
            legacy_asset_count,
            revision: 0,
            updated_at: None,
            updated_by: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectStorageSettingsInput {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    pub key_prefix: String,
    pub path_style: bool,
    pub public_base_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestObjectStorageSettings {
    #[serde(flatten)]
    pub settings: ObjectStorageSettingsInput,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateObjectStorageSettings {
    #[serde(flatten)]
    pub settings: ObjectStorageSettingsInput,
    pub adopt_legacy_assets: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectStorageTestResult {
    pub ok: bool,
    pub public_url: String,
}
