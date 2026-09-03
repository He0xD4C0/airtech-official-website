pub const SESSION_COOKIE: &str = "airtek_admin_session";
pub const CSRF_COOKIE: &str = "airtek_admin_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";
const SESSION_HOURS: i64 = 12;
const SESSION_IDLE_MINUTES: i64 = 30;
const RATE_WINDOW_MINUTES: i64 = 10;
const RATE_BLOCK_MINUTES: i64 = 15;
const RATE_MAX_FAILURES: u32 = 5;
const RATE_MAX_MEMORY_KEYS: usize = 10_000;
static DUMMY_PASSWORD_HASH: OnceLock<String> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct AuthRateLimit {
    pub attempts: u32,
    pub window_started_at: DateTime<Utc>,
    pub blocked_until: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

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
pub struct StoredSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: Vec<u8>,
    pub csrf_hash: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

#[derive(Clone, Debug)]
pub struct StoredRecoveryCode {
    pub hash: String,
    pub used: bool,
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
}

impl AdminPrincipal {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.totp_enabled && self.permissions.iter().any(|value| value == permission)
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
            permissions: if self.totp_enabled {
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
struct SetupRequest {
    display_name: String,
    email: String,
    password: String,
    bootstrap_token: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
    otp: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AcceptInvitationRequest {
    token: String,
    password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationAcceptance {
    user_id: Uuid,
    email: String,
    display_name: String,
    locale: String,
    role_keys: Vec<String>,
    status: &'static str,
    accepted_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct TotpCodeRequest {
    code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TotpEnrollment {
    secret: String,
    otp_auth_uri: String,
    algorithm: &'static str,
    digits: u8,
    period_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryCodeSet {
    recovery_codes: Vec<String>,
    generated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSummary {
    id: Uuid,
    current: bool,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

struct SessionIssue {
    principal: AdminPrincipal,
    session_token: String,
    csrf_token: String,
}

#[derive(Clone, Copy)]
enum SecondFactorMethod {
    Totp,
    RecoveryCode,
}

impl SecondFactorMethod {
    fn label(self) -> &'static str {
        match self {
            Self::Totp => "totp",
            Self::RecoveryCode => "recoveryCode",
        }
    }
}
