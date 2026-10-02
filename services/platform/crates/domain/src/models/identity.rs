use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserRecord {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub locale: String,
    pub status: String,
    pub revision: i64,
    pub manager_user_id: Option<Uuid>,
    pub roles: Vec<String>,
    pub totp_enabled: bool,
    pub invited_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminUserPage {
    pub items: Vec<AdminUserRecord>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminRoleRecord {
    pub id: Uuid,
    pub key: String,
    pub display_name: String,
    pub system_role: bool,
    pub is_preset: bool,
    pub revision: i64,
    pub permissions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminRolePage {
    pub items: Vec<AdminRoleRecord>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAdminRole {
    pub display_name: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAdminRole {
    pub key: String,
    pub display_name: String,
    pub permissions: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InviteAdminUser {
    pub email: String,
    pub display_name: String,
    pub role_keys: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInvitation {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub locale: String,
    pub role_keys: Vec<String>,
    pub status: String,
    pub invited_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Returned only from invitation creation because email delivery is not
    /// connected in phase one. Only its SHA-256 digest is stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitation_token: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAdminUser {
    pub display_name: Option<String>,
    pub locale: Option<String>,
    pub status: Option<String>,
    pub role_keys: Option<Vec<String>>,
    pub manager_user_id: Option<Option<Uuid>>,
    pub reason: String,
}
