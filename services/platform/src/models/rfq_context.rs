use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RfqQuantity {
    Integer(u64),
    Text(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RfqDutyPoint {
    pub airflow: f64,
    pub airflow_unit: String,
    pub pressure: f64,
    pub pressure_unit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RfqElectricalContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_hz: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductRfqContext {
    pub application: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<RfqQuantity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrical: Option<RfqElectricalContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionRfqContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ambient_temperature_c: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_family: Option<ProductFamily>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motor_technology: Option<String>,
    pub application: String,
    pub duty_point: RfqDutyPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<RfqQuantity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrical: Option<RfqElectricalContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum_diameter_mm: Option<f64>,
    #[serde(default)]
    pub required_certifications: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRfqContext {
    pub application: String,
    pub project_stage: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<RfqQuantity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrical: Option<RfqElectricalContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_scale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engineering_needs: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplacementRfqContext {
    pub application: String,
    pub existing_model: String,
    pub duty_point: RfqDutyPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<RfqQuantity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electrical: Option<RfqElectricalContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation_constraints: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_goal: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "journey", content = "context", rename_all = "camelCase")]
pub enum RfqContextSnapshot {
    Product(ProductRfqContext),
    Selection(SelectionRfqContext),
    Project(ProjectRfqContext),
    Replacement(ReplacementRfqContext),
}
