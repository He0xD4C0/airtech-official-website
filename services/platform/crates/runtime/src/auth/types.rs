use super::*;

pub const SESSION_COOKIE: &str = "airtek_admin_session";
pub const CSRF_COOKIE: &str = "airtek_admin_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";
pub(super) const SESSION_HOURS: i64 = 24;
pub(super) const SESSION_IDLE_MINUTES: i64 = 60;
pub(super) const RATE_WINDOW_MINUTES: i64 = 10;
pub(super) const RATE_BLOCK_MINUTES: i64 = 15;
pub(super) const RATE_MAX_FAILURES: u32 = 5;
pub(super) static DUMMY_PASSWORD_HASH: OnceLock<String> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct StoredUser {
    pub id: Uuid,
    pub display_name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub role_keys: Vec<String>,
    pub permissions: Vec<String>,
    pub active: bool,
    pub totp_enabled: bool,
    pub totp_secret_ciphertext: Option<Vec<u8>>,
    pub must_change_password: bool,
    pub must_confirm_recovery_key: bool,
    pub phone_verified: bool,
}

#[derive(Clone, Debug)]
pub struct AdminPrincipal {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub role_keys: Vec<String>,
    pub permissions: Vec<String>,
    pub session_id: Uuid,
    pub session_token_hash: Vec<u8>,
    pub csrf_hash: Vec<u8>,
    pub totp_enabled: bool,
    pub must_change_password: bool,
    pub must_confirm_recovery_key: bool,
    pub phone_verified: bool,
}

impl AdminPrincipal {
    pub fn is_super_admin(&self) -> bool {
        self.role_keys.iter().any(|key| key == "super-admin")
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn session_user(&self, state: &AppState) -> SessionUser {
        SessionUser {
            id: self.user_id,
            display_name: self.display_name.clone(),
            email: self.email.clone(),
            role: self.role.clone(),
            role_keys: self.role_keys.clone(),
            permissions: self.permissions.clone(),
            environment: state.environment_label().into(),
            totp_enabled: self.totp_enabled,
            phone_verified: self.phone_verified,
            must_change_password: self.must_change_password,
            must_confirm_recovery_key: self.must_confirm_recovery_key,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUser {
    pub id: Uuid,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub role_keys: Vec<String>,
    pub permissions: Vec<String>,
    pub environment: String,
    pub totp_enabled: bool,
    pub phone_verified: bool,
    pub must_change_password: bool,
    pub must_confirm_recovery_key: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequest {
    pub(super) display_name: String,
    pub(super) email: String,
    pub(super) password: String,
    pub(super) bootstrap_token: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub(super) email: String,
    pub(super) password: String,
    pub(super) otp: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptInvitationRequest {
    pub(super) token: String,
    pub(super) password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InvitationAcceptance {
    pub(super) user_id: Uuid,
    pub(super) email: String,
    pub(super) display_name: String,
    pub(super) locale: String,
    pub(super) role_keys: Vec<String>,
    pub(super) status: &'static str,
    pub(super) accepted_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct TotpCodeRequest {
    pub(super) code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpEnrollment {
    pub(super) secret: String,
    pub(super) otp_auth_uri: String,
    pub(super) algorithm: &'static str,
    pub(super) digits: u8,
    pub(super) period_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCodeSet {
    pub(super) recovery_codes: Vec<String>,
    pub(super) generated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub(super) id: Uuid,
    pub(super) current: bool,
    pub(super) created_at: DateTime<Utc>,
    pub(super) last_seen_at: DateTime<Utc>,
    pub(super) expires_at: DateTime<Utc>,
}

pub(super) struct SessionIssue {
    pub(super) principal: AdminPrincipal,
    pub(super) session_token: String,
    pub(super) csrf_token: String,
}

#[derive(Clone, Copy)]
pub(super) enum SecondFactorMethod {
    Totp,
    RecoveryCode,
}

impl SecondFactorMethod {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Totp => "totp",
            Self::RecoveryCode => "recoveryCode",
        }
    }
}
