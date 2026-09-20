use super::*;

pub const SESSION_COOKIE: &str = "airtek_admin_session";
pub const CSRF_COOKIE: &str = "airtek_admin_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";
pub(super) const SESSION_HOURS: i64 = 12;
pub(super) const SESSION_IDLE_MINUTES: i64 = 30;
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
    pub permissions: Vec<String>,
    pub active: bool,
    pub totp_enabled: bool,
    pub totp_secret_ciphertext: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct AdminPrincipal {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub session_id: Uuid,
    pub session_token_hash: Vec<u8>,
    pub csrf_hash: Vec<u8>,
    pub totp_enabled: bool,
    pub development_password_only: bool,
}

impl AdminPrincipal {
    pub fn business_access_enabled(&self) -> bool {
        self.totp_enabled || self.development_password_only
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.business_access_enabled() && self.permissions.iter().any(|value| value == permission)
    }

    pub fn session_user(&self, state: &AppState) -> SessionUser {
        SessionUser {
            id: self.user_id,
            display_name: self.display_name.clone(),
            email: self.email.clone(),
            role: self.role.clone(),
            // Password-only sessions exist solely to finish first-login TOTP
            // enrollment. Do not expose or authorize business permissions
            // until the second factor has been confirmed.
            permissions: if self.business_access_enabled() {
                self.permissions.clone()
            } else {
                Vec::new()
            },
            environment: state.environment_label().into(),
            totp_enabled: self.totp_enabled,
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
    pub permissions: Vec<String>,
    pub environment: String,
    pub totp_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetupRequest {
    pub(super) display_name: String,
    pub(super) email: String,
    pub(super) password: String,
    pub(super) bootstrap_token: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct LoginRequest {
    pub(super) email: String,
    pub(super) password: String,
    pub(super) otp: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AcceptInvitationRequest {
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
pub(super) struct TotpCodeRequest {
    pub(super) code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TotpEnrollment {
    pub(super) secret: String,
    pub(super) otp_auth_uri: String,
    pub(super) algorithm: &'static str,
    pub(super) digits: u8,
    pub(super) period_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RecoveryCodeSet {
    pub(super) recovery_codes: Vec<String>,
    pub(super) generated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SessionSummary {
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
