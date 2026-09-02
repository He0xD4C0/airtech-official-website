use std::{net::SocketAddr, sync::OnceLock};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{connect_info::ConnectInfo, Extension, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::AuditEvent,
    second_factor::{generate_recovery_codes, normalize_recovery_code, EnrollmentSecret},
    state::AppState,
};

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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/invitations/accept", post(accept_invitation))
        .route("/auth/session", get(session))
        .route("/auth/logout", post(logout))
        .route("/auth/totp/enrollment", post(start_totp_enrollment))
        .route("/auth/totp/confirm", post(confirm_totp_enrollment))
        .route(
            "/auth/recovery-codes/regenerate",
            post(regenerate_recovery_codes),
        )
        .route("/auth/sessions", get(list_sessions))
        .route(
            "/auth/sessions/{id}",
            axum::routing::delete(revoke_session_by_id),
        )
}

async fn accept_invitation(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<AcceptInvitationRequest>,
) -> Result<Response, ApiError> {
    validate_strong_password(&request.password)?;
    let token_digest = token_hash(&request.token);
    let account_key = hex_digest(&token_digest);
    let rate_keys = rate_limit_keys(
        "invitation-accept",
        &request_source(&state, &headers, peer),
        &account_key,
    );
    check_auth_rate_limits(&state, &rate_keys).await?;
    if request.token.len() != 43
        || URL_SAFE_NO_PAD
            .decode(request.token.as_bytes())
            .ok()
            .is_none_or(|decoded| decoded.len() != 32)
    {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    }

    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Invitation acceptance requires PostgreSQL persistence.",
        ));
    };
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let password_hash = hash_password(&request.password)?;
    let now = Utc::now();
    let request_id = request_id(&headers);
    let mut transaction = pool.begin().await?;
    let invitation = sqlx::query(
        r#"SELECT id,email,display_name,locale,status,invited_by,invited_at,expires_at
           FROM user_invitations WHERE token_hash=$1 FOR UPDATE"#,
    )
    .bind(&token_digest)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(invitation) = invitation else {
        transaction.rollback().await?;
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    };
    let invitation_id: Uuid = invitation.try_get("id")?;
    let status: String = invitation.try_get("status")?;
    let expires_at: DateTime<Utc> = invitation.try_get("expires_at")?;
    if status != "pending" || expires_at <= now {
        if status == "pending" && expires_at <= now {
            sqlx::query(
                "UPDATE user_invitations SET status='expired' WHERE id=$1 AND status='pending'",
            )
            .bind(invitation_id)
            .execute(&mut *transaction)
            .await?;
            transaction.commit().await?;
        } else {
            transaction.rollback().await?;
        }
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    }

    let email: String = invitation.try_get("email")?;
    let display_name: String = invitation.try_get("display_name")?;
    let locale: String = invitation.try_get("locale")?;
    let invited_by: Uuid = invitation.try_get("invited_by")?;
    let invited_at: DateTime<Utc> = invitation.try_get("invited_at")?;
    let email_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($1))")
            .bind(&email)
            .fetch_one(&mut *transaction)
            .await?;
    if email_exists {
        transaction.rollback().await?;
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::conflict(
            "An administrator account already exists for this invitation email.",
        ));
    }

    let role_keys = sqlx::query_scalar::<_, String>(
        r#"SELECT role.key FROM user_invitation_roles assignment
           JOIN roles role ON role.id=assignment.role_id
           WHERE assignment.invitation_id=$1 ORDER BY role.key"#,
    )
    .bind(invitation_id)
    .fetch_all(&mut *transaction)
    .await?;
    if role_keys.is_empty() {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "The invitation has no assignable administrator role.",
        ));
    }

    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO users
           (id,email,password_hash,display_name,locale,status,invited_by,invited_at,created_at,updated_at)
           VALUES ($1,$2,$3,$4,$5,'active',$6,$7,$8,$8)"#,
    )
    .bind(user_id)
    .bind(&email)
    .bind(&password_hash)
    .bind(&display_name)
    .bind(&locale)
    .bind(invited_by)
    .bind(invited_at)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO user_roles(user_id,role_id)
           SELECT $1,role_id FROM user_invitation_roles WHERE invitation_id=$2"#,
    )
    .bind(user_id)
    .bind(invitation_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO user_status_history
           (id,user_id,from_status,to_status,reason,changed_by,request_id,changed_at)
           VALUES ($1,$2,'invited','active','Accepted administrator invitation',$3,$4,$5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(invited_by)
    .bind(request_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    let updated = sqlx::query(
        r#"UPDATE user_invitations
           SET status='accepted',accepted_at=$2,accepted_user_id=$3
           WHERE id=$1 AND status='pending' AND expires_at>$2"#,
    )
    .bind(invitation_id)
    .bind(now)
    .bind(user_id)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() != 1 {
        transaction.rollback().await?;
        record_auth_failures(&state, &rate_keys).await?;
        return Err(invalid_invitation());
    }
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,request_id,occurred_at)
           VALUES ($1,'invitation-acceptance','identity.invitation.accept','userInvitation',$2,
                   $3,$4,'Accept administrator invitation',$5,$6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(invitation_id)
    .bind(json!({"status": "pending"}))
    .bind(json!({
        "status": "accepted",
        "acceptedUserId": user_id,
        "roleKeys": role_keys,
    }))
    .bind(request_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;

    let mut response = (
        StatusCode::CREATED,
        Json(InvitationAcceptance {
            user_id,
            email,
            display_name,
            locale,
            role_keys,
            status: "active",
            accepted_at: now,
        }),
    )
        .into_response();
    append_no_store(response.headers_mut());
    Ok(response)
}

fn invalid_invitation() -> ApiError {
    ApiError::unauthorized("The invitation token is invalid, expired, revoked, or already used.")
}

async fn setup(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<SetupRequest>,
) -> Result<Response, ApiError> {
    validate_setup(&request)?;
    let rate_keys = rate_limit_keys(
        "setup",
        &request_source(&state, &headers, peer),
        &request.email,
    );
    check_auth_rate_limits(&state, &rate_keys).await?;
    if has_admin_users(&state).await? {
        return Err(ApiError::conflict(
            "Initial setup is disabled after the first user is created.",
        ));
    }
    if let Err(error) = verify_bootstrap_token(&state, &request.bootstrap_token) {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(error);
    }
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let password_hash = hash_password(&request.password)?;
    let user = create_initial_super_admin(
        &state,
        request.display_name.trim(),
        request.email.trim(),
        password_hash,
    )
    .await?;
    let issue = create_session(&state, user).await?;
    record_auth_audit(
        &state,
        request.email.trim(),
        "auth.initial_setup",
        issue.principal.user_id,
        json!({"role": "Super Admin"}),
        &headers,
    )
    .await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    Ok(session_response(&state, issue, StatusCode::CREATED))
}

async fn has_admin_users(state: &AppState) -> Result<bool, ApiError> {
    if let Some(pool) = &state.pool {
        return Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
            .fetch_one(pool)
            .await?);
    }
    Ok(!state.data.read().await.admin_users.is_empty())
}

async fn login(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let email = request.email.trim().to_ascii_lowercase();
    let rate_keys = rate_limit_keys("login", &request_source(&state, &headers, peer), &email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    let user = find_user_by_email(&state, &email).await?;
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let valid = if let Some(user) = &user {
        user.active && verify_password(&user.password_hash, &request.password)
    } else {
        // Make unknown-user login perform Argon2 work too. This is not intended
        // to be perfectly identical timing, but avoids the trivial fast path.
        let dummy = DUMMY_PASSWORD_HASH.get_or_init(|| {
            hash_password("invalid-login-password")
                .expect("the fixed Argon2 dummy password must be hashable")
        });
        let _ = verify_password(dummy, &request.password);
        false
    };
    if !valid {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::unauthorized("Email or password is incorrect."));
    }
    let user = user.expect("valid login has a user");
    let second_factor = if user.totp_enabled {
        let Some(code) = request
            .otp
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
        else {
            record_auth_failures(&state, &rate_keys).await?;
            return Err(ApiError::unauthorized(
                "A valid TOTP or recovery code is required.",
            ));
        };
        let Some(method) = verify_login_second_factor(&state, &user, code).await? else {
            record_auth_failures(&state, &rate_keys).await?;
            return Err(ApiError::unauthorized(
                "A valid TOTP or recovery code is required.",
            ));
        };
        Some(method)
    } else {
        None
    };
    let issue = create_session(&state, user).await?;
    record_auth_audit(
        &state,
        &issue.principal.email,
        "auth.login",
        issue.principal.user_id,
        json!({"secondFactor": second_factor.map(SecondFactorMethod::label).unwrap_or("none")}),
        &headers,
    )
    .await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    Ok(session_response(&state, issue, StatusCode::OK))
}

async fn session(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let csrf_token = random_token();
    rotate_csrf(&state, &principal, &csrf_token).await?;
    let refreshed = AdminPrincipal {
        csrf_hash: token_hash(&csrf_token),
        ..principal
    };
    Ok(session_refresh_response(&state, refreshed, csrf_token))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, ApiError> {
    if cookie_value(&headers, SESSION_COOKIE).is_some() {
        match authenticate(&state, &headers).await {
            Ok(principal) => {
                verify_csrf(&headers, &principal)?;
                revoke_session(&state, &principal).await?;
                record_auth_audit(
                    &state,
                    &principal.email,
                    "auth.logout",
                    principal.session_id,
                    json!({"currentSession": true}),
                    &headers,
                )
                .await?;
            }
            Err(error) if error.status() == StatusCode::UNAUTHORIZED => {}
            Err(error) => return Err(error),
        }
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    append_clear_cookies(&state, response.headers_mut());
    Ok(response)
}

async fn start_totp_enrollment(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    if principal.totp_enabled {
        return Err(ApiError::conflict(
            "TOTP is already enabled for this account.",
        ));
    }
    let key = totp_encryption_key(&state)?;
    let secret = EnrollmentSecret::generate()?;
    let ciphertext = secret.seal(key, principal.user_id)?;
    if let Some(pool) = &state.pool {
        let result = sqlx::query(
            "UPDATE users SET totp_secret_ciphertext=$1, updated_at=now() WHERE id=$2 AND totp_confirmed_at IS NULL",
        )
        .bind(&ciphertext)
        .bind(principal.user_id)
        .execute(pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "TOTP is already enabled for this account.",
            ));
        }
    } else {
        let mut data = state.data.write().await;
        let user = data
            .admin_users
            .get_mut(&principal.user_id)
            .ok_or_else(|| ApiError::unauthorized("The admin account is unavailable."))?;
        if user.totp_enabled {
            return Err(ApiError::conflict(
                "TOTP is already enabled for this account.",
            ));
        }
        user.totp_secret_ciphertext = Some(ciphertext);
    }
    record_auth_audit(
        &state,
        &principal.email,
        "auth.totp.enrollment_started",
        principal.user_id,
        json!({"totpEnabled": false}),
        &headers,
    )
    .await?;
    Ok(sensitive_json(TotpEnrollment {
        secret: secret.base32(),
        otp_auth_uri: secret.provisioning_uri(&principal.email)?,
        algorithm: "SHA1",
        digits: 6,
        period_seconds: 30,
    }))
}

async fn confirm_totp_enrollment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TotpCodeRequest>,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    if principal.totp_enabled {
        return Err(ApiError::conflict(
            "TOTP is already enabled for this account.",
        ));
    }
    let rate_keys = rate_limit_keys("totp-confirm", "authenticated-session", &principal.email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    if !verify_pending_totp(
        &state,
        principal.user_id,
        &principal.email,
        request.code.trim(),
    )
    .await?
    {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::unauthorized(
            "The TOTP code is invalid or expired.",
        ));
    }
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let (codes, hashes) = new_recovery_code_set()?;
    persist_confirmed_totp(&state, principal.user_id, &hashes).await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.totp.enabled",
        principal.user_id,
        json!({"totpEnabled": true, "recoveryCodeCount": codes.len()}),
        &headers,
    )
    .await?;
    Ok(sensitive_json(RecoveryCodeSet {
        recovery_codes: codes,
        generated_at: Utc::now(),
    }))
}

async fn regenerate_recovery_codes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<TotpCodeRequest>,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    if !principal.totp_enabled {
        return Err(ApiError::forbidden(
            "TOTP must be enabled before recovery codes can be regenerated.",
        ));
    }
    let rate_keys = rate_limit_keys("totp-regenerate", "authenticated-session", &principal.email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    if !verify_principal_totp(&state, &principal, request.code.trim()).await? {
        record_auth_failures(&state, &rate_keys).await?;
        return Err(ApiError::unauthorized(
            "The TOTP code is invalid or expired.",
        ));
    }
    let _hash_slot = state
        .auth_hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::too_many_requests("Authentication capacity is temporarily full."))?;
    let (codes, hashes) = new_recovery_code_set()?;
    replace_recovery_codes(&state, principal.user_id, &hashes).await?;
    clear_auth_rate_limits(&state, &rate_keys).await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.recovery_codes.regenerated",
        principal.user_id,
        json!({"recoveryCodeCount": codes.len()}),
        &headers,
    )
    .await?;
    Ok(sensitive_json(RecoveryCodeSet {
        recovery_codes: codes,
        generated_at: Utc::now(),
    }))
}

async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate(&state, &headers).await?;
    let now = Utc::now();
    let mut sessions = if let Some(pool) = &state.pool {
        sqlx::query(
            r#"SELECT id, created_at, last_seen_at, expires_at
               FROM sessions WHERE user_id=$1 AND revoked_at IS NULL
               AND expires_at > now() AND last_seen_at > now() - interval '30 minutes'
               ORDER BY last_seen_at DESC"#,
        )
        .bind(principal.user_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            let id: Uuid = row.try_get("id")?;
            Ok(SessionSummary {
                id,
                current: id == principal.session_id,
                created_at: row.try_get("created_at")?,
                last_seen_at: row.try_get("last_seen_at")?,
                expires_at: row.try_get("expires_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?
    } else {
        state
            .data
            .read()
            .await
            .admin_sessions
            .values()
            .filter(|session| {
                session.user_id == principal.user_id
                    && !session.revoked
                    && session.expires_at > now
                    && session.last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) > now
            })
            .map(|session| SessionSummary {
                id: session.id,
                current: session.id == principal.session_id,
                created_at: session.created_at,
                last_seen_at: session.last_seen_at,
                expires_at: session.expires_at,
            })
            .collect()
    };
    sessions.sort_by_key(|session| std::cmp::Reverse(session.last_seen_at));
    Ok(sensitive_json(sessions))
}

async fn revoke_session_by_id(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    let revoked = if let Some(pool) = &state.pool {
        sqlx::query(
            "UPDATE sessions SET revoked_at=now() WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(principal.user_id)
        .execute(pool)
        .await?
        .rows_affected()
            == 1
    } else {
        let mut data = state.data.write().await;
        data.admin_sessions
            .get_mut(&session_id)
            .filter(|session| session.user_id == principal.user_id && !session.revoked)
            .map(|session| session.revoked = true)
            .is_some()
    };
    if !revoked {
        return Err(ApiError::not_found("The active session was not found."));
    }
    record_auth_audit(
        &state,
        &principal.email,
        "auth.session.revoked",
        session_id,
        json!({"currentSession": session_id == principal.session_id}),
        &headers,
    )
    .await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    if session_id == principal.session_id {
        append_clear_cookies(&state, response.headers_mut());
    }
    Ok(response)
}

pub async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, ApiError> {
    let token = cookie_value(headers, SESSION_COOKIE)
        .ok_or_else(|| ApiError::unauthorized("An active admin session is required."))?;
    let supplied_hash = token_hash(&token);

    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT session.id AS session_id, session.user_id, session.csrf_hash,
                      session.expires_at, session.last_seen_at, user_account.display_name, user_account.email,
                      user_account.status, user_account.totp_confirmed_at
               FROM sessions AS session
               JOIN users AS user_account ON user_account.id = session.user_id
               WHERE session.token_hash = $1 AND session.revoked_at IS NULL"#,
        )
        .bind(&supplied_hash)
        .fetch_optional(pool)
        .await?;
        let row = row.ok_or_else(|| ApiError::unauthorized("The admin session is invalid."))?;
        let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
        let last_seen_at: DateTime<Utc> = row.try_get("last_seen_at")?;
        let status: String = row.try_get("status")?;
        let now = Utc::now();
        if expires_at <= now
            || last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) <= now
            || status != "active"
        {
            sqlx::query("UPDATE sessions SET revoked_at=COALESCE(revoked_at, now()) WHERE id=$1")
                .bind(row.try_get::<Uuid, _>("session_id")?)
                .execute(pool)
                .await?;
            return Err(ApiError::unauthorized("The admin session has expired."));
        }
        let user_id: Uuid = row.try_get("user_id")?;
        let (role, permissions) = load_roles_and_permissions(pool, user_id).await?;
        sqlx::query("UPDATE sessions SET last_seen_at=now() WHERE id=$1")
            .bind(row.try_get::<Uuid, _>("session_id")?)
            .execute(pool)
            .await?;
        return Ok(AdminPrincipal {
            user_id,
            display_name: row.try_get("display_name")?,
            email: row.try_get("email")?,
            role,
            permissions,
            session_id: row.try_get("session_id")?,
            session_token_hash: supplied_hash,
            csrf_hash: row.try_get("csrf_hash")?,
            totp_enabled: row
                .try_get::<Option<DateTime<Utc>>, _>("totp_confirmed_at")?
                .is_some(),
        });
    }

    let (stored_session, user) = {
        let data = state.data.read().await;
        let now = Utc::now();
        let stored_session = data
            .admin_sessions
            .values()
            .find(|session| {
                !session.revoked
                    && session.expires_at > now
                    && session.last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) > now
                    && constant_time_equal(&session.token_hash, &supplied_hash)
            })
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("The admin session is invalid or expired."))?;
        let user = data
            .admin_users
            .get(&stored_session.user_id)
            .filter(|user| user.active)
            .cloned()
            .ok_or_else(|| ApiError::unauthorized("The admin account is disabled."))?;
        (stored_session, user)
    };
    if let Some(session) = state
        .data
        .write()
        .await
        .admin_sessions
        .get_mut(&stored_session.id)
    {
        session.last_seen_at = Utc::now();
    }
    Ok(AdminPrincipal {
        user_id: user.id,
        display_name: user.display_name,
        email: user.email,
        role: user.role,
        permissions: user.permissions,
        session_id: stored_session.id,
        session_token_hash: stored_session.token_hash,
        csrf_hash: stored_session.csrf_hash,
        totp_enabled: user.totp_enabled,
    })
}

/// Revalidate the identity and authorization bound into a signed preview token.
///
/// The token is only a short-lived transport credential: disabling the user,
/// revoking or expiring the issuing session, or removing `content.read` must
/// invalidate the preview immediately. Production always takes the PostgreSQL
/// branch because the API binary refuses to start without `DATABASE_URL`.
pub async fn preview_session_is_authorized(
    state: &AppState,
    user_id: Uuid,
    session_id: Uuid,
) -> Result<bool, ApiError> {
    if user_id.is_nil() || session_id.is_nil() {
        return Ok(false);
    }
    if let Some(pool) = &state.pool {
        return Ok(sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS (
                   SELECT 1
                   FROM sessions AS session
                   JOIN users AS user_account ON user_account.id=session.user_id
                   JOIN user_roles AS assignment ON assignment.user_id=user_account.id
                   JOIN role_permissions AS role_permission
                     ON role_permission.role_id=assignment.role_id
                   WHERE session.id=$1
                     AND session.user_id=$2
                     AND session.revoked_at IS NULL
                     AND session.expires_at > now()
                     AND session.last_seen_at + ($3::bigint * interval '1 minute') > now()
                     AND user_account.status='active'
                     AND user_account.totp_confirmed_at IS NOT NULL
                     AND role_permission.permission_key='content.read'
               )"#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(SESSION_IDLE_MINUTES)
        .fetch_one(pool)
        .await?);
    }

    let data = state.data.read().await;
    let now = Utc::now();
    let Some(session) = data.admin_sessions.get(&session_id).filter(|session| {
        session.user_id == user_id
            && !session.revoked
            && session.expires_at > now
            && session.last_seen_at + Duration::minutes(SESSION_IDLE_MINUTES) > now
    }) else {
        return Ok(false);
    };
    Ok(data.admin_users.get(&session.user_id).is_some_and(|user| {
        user.active
            && user.totp_enabled
            && user.permissions.iter().any(|value| value == "content.read")
    }))
}

async fn authenticate_with_csrf(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, ApiError> {
    let principal = authenticate(state, headers).await?;
    verify_csrf(headers, &principal)?;
    Ok(principal)
}

fn totp_encryption_key(state: &AppState) -> Result<&crate::config::TotpEncryptionKey, ApiError> {
    state.config.totp_encryption_key.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(
            "TOTP is unavailable until AIRTEK_TOTP_ENCRYPTION_KEY is configured.",
        )
    })
}

async fn stored_totp_ciphertext(state: &AppState, user_id: Uuid) -> Result<Vec<u8>, ApiError> {
    if let Some(pool) = &state.pool {
        return sqlx::query_scalar::<_, Option<Vec<u8>>>(
            "SELECT totp_secret_ciphertext FROM users WHERE id=$1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .flatten()
        .ok_or_else(|| ApiError::bad_request("Start TOTP enrollment before confirming it."));
    }
    state
        .data
        .read()
        .await
        .admin_users
        .get(&user_id)
        .and_then(|user| user.totp_secret_ciphertext.clone())
        .ok_or_else(|| ApiError::bad_request("Start TOTP enrollment before confirming it."))
}

async fn verify_pending_totp(
    state: &AppState,
    user_id: Uuid,
    account: &str,
    code: &str,
) -> Result<bool, ApiError> {
    let ciphertext = stored_totp_ciphertext(state, user_id).await?;
    EnrollmentSecret::open(&ciphertext, totp_encryption_key(state)?, user_id)?
        .verify_current(code, account)
}

pub async fn verify_totp_reauthentication(
    state: &AppState,
    principal: &AdminPrincipal,
    code: &str,
) -> Result<(), ApiError> {
    if !principal.totp_enabled {
        return Err(ApiError::forbidden(
            "This account must enable TOTP before running high-risk operations.",
        ));
    }
    let rate_keys = rate_limit_keys("totp-reauth", "authenticated-session", &principal.email);
    check_auth_rate_limits(state, &rate_keys).await?;
    if !verify_principal_totp(state, principal, code.trim()).await? {
        record_auth_failures(state, &rate_keys).await?;
        return Err(ApiError::forbidden("A valid X-TOTP-Code is required."));
    }
    clear_auth_rate_limits(state, &rate_keys).await?;
    Ok(())
}

async fn verify_principal_totp(
    state: &AppState,
    principal: &AdminPrincipal,
    code: &str,
) -> Result<bool, ApiError> {
    verify_pending_totp(state, principal.user_id, &principal.email, code).await
}

async fn verify_login_second_factor(
    state: &AppState,
    user: &StoredUser,
    code: &str,
) -> Result<Option<SecondFactorMethod>, ApiError> {
    if code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit()) {
        let ciphertext = user.totp_secret_ciphertext.as_deref().ok_or_else(|| {
            ApiError::service_unavailable("The confirmed TOTP secret is unavailable.")
        })?;
        let valid = EnrollmentSecret::open(ciphertext, totp_encryption_key(state)?, user.id)?
            .verify_current(code, &user.email)?;
        return Ok(valid.then_some(SecondFactorMethod::Totp));
    }
    let Some(normalized) = normalize_recovery_code(code) else {
        return Ok(None);
    };
    Ok(consume_recovery_code(state, user.id, &normalized)
        .await?
        .then_some(SecondFactorMethod::RecoveryCode))
}

async fn consume_recovery_code(
    state: &AppState,
    user_id: Uuid,
    normalized: &str,
) -> Result<bool, ApiError> {
    if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            "SELECT code_hash FROM recovery_codes WHERE user_id=$1 AND used_at IS NULL",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?;
        for row in rows {
            let encoded = row.try_get::<Vec<u8>, _>("code_hash")?;
            let Ok(encoded) = String::from_utf8(encoded) else {
                continue;
            };
            if verify_password(&encoded, normalized) {
                let consumed = sqlx::query(
                    "UPDATE recovery_codes SET used_at=now() WHERE user_id=$1 AND code_hash=$2 AND used_at IS NULL",
                )
                .bind(user_id)
                .bind(encoded.as_bytes())
                .execute(pool)
                .await?
                .rows_affected()
                    == 1;
                return Ok(consumed);
            }
        }
        return Ok(false);
    }
    let mut data = state.data.write().await;
    let Some(codes) = data.recovery_codes.get_mut(&user_id) else {
        return Ok(false);
    };
    for code in codes.iter_mut().filter(|code| !code.used) {
        if verify_password(&code.hash, normalized) {
            code.used = true;
            return Ok(true);
        }
    }
    Ok(false)
}

fn new_recovery_code_set() -> Result<(Vec<String>, Vec<String>), ApiError> {
    let codes = generate_recovery_codes()?;
    let hashes = codes
        .iter()
        .map(|code| {
            normalize_recovery_code(code)
                .ok_or_else(|| ApiError::internal("Generated recovery code is invalid."))
                .and_then(|normalized| hash_password(&normalized))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((codes, hashes))
}

async fn persist_confirmed_totp(
    state: &AppState,
    user_id: Uuid,
    hashes: &[String],
) -> Result<(), ApiError> {
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE users SET totp_confirmed_at=now(), updated_at=now()
               WHERE id=$1 AND totp_confirmed_at IS NULL AND totp_secret_ciphertext IS NOT NULL"#,
        )
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "TOTP enrollment was already confirmed or is unavailable.",
            ));
        }
        replace_recovery_codes_transaction(&mut transaction, user_id, hashes).await?;
        transaction.commit().await?;
        return Ok(());
    }
    let mut data = state.data.write().await;
    let user = data
        .admin_users
        .get_mut(&user_id)
        .ok_or_else(|| ApiError::unauthorized("The admin account is unavailable."))?;
    if user.totp_enabled || user.totp_secret_ciphertext.is_none() {
        return Err(ApiError::conflict(
            "TOTP enrollment was already confirmed or is unavailable.",
        ));
    }
    user.totp_enabled = true;
    data.recovery_codes.insert(
        user_id,
        hashes
            .iter()
            .map(|hash| StoredRecoveryCode {
                hash: hash.clone(),
                used: false,
            })
            .collect(),
    );
    Ok(())
}

async fn replace_recovery_codes(
    state: &AppState,
    user_id: Uuid,
    hashes: &[String],
) -> Result<(), ApiError> {
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        replace_recovery_codes_transaction(&mut transaction, user_id, hashes).await?;
        transaction.commit().await?;
    } else {
        state.data.write().await.recovery_codes.insert(
            user_id,
            hashes
                .iter()
                .map(|hash| StoredRecoveryCode {
                    hash: hash.clone(),
                    used: false,
                })
                .collect(),
        );
    }
    Ok(())
}

async fn replace_recovery_codes_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Uuid,
    hashes: &[String],
) -> Result<(), ApiError> {
    sqlx::query("DELETE FROM recovery_codes WHERE user_id=$1")
        .bind(user_id)
        .execute(&mut **transaction)
        .await?;
    for hash in hashes {
        sqlx::query("INSERT INTO recovery_codes (user_id, code_hash) VALUES ($1,$2)")
            .bind(user_id)
            .bind(hash.as_bytes())
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

async fn record_auth_audit(
    state: &AppState,
    actor: &str,
    action: &str,
    entity_id: Uuid,
    after: serde_json::Value,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    state
        .persist_audit(AuditEvent {
            id: Uuid::new_v4(),
            actor: actor.into(),
            action: action.into(),
            entity_type: if action.contains("session") {
                "session".into()
            } else {
                "user".into()
            },
            entity_id: Some(entity_id),
            before: None,
            after: Some(after),
            reason: None,
            request_id,
            occurred_at: Utc::now(),
        })
        .await
}

pub fn verify_csrf(headers: &HeaderMap, principal: &AdminPrincipal) -> Result<(), ApiError> {
    let token = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::forbidden("X-CSRF-Token is required for this request."))?;
    let supplied_hash = token_hash(token);
    if !constant_time_equal(&supplied_hash, &principal.csrf_hash) {
        return Err(ApiError::forbidden("The CSRF token is invalid or expired."));
    }
    Ok(())
}

fn validate_setup(request: &SetupRequest) -> Result<(), ApiError> {
    if request.display_name.trim().is_empty() || request.display_name.trim().len() > 120 {
        return Err(ApiError::bad_request(
            "displayName must contain 1 to 120 characters.",
        ));
    }
    if !valid_email(request.email.trim()) {
        return Err(ApiError::bad_request("A valid work email is required."));
    }
    validate_strong_password(&request.password)?;
    Ok(())
}

fn validate_strong_password(password: &str) -> Result<(), ApiError> {
    if password.len() < 12
        || password.len() > 256
        || !password.chars().any(|value| value.is_ascii_alphabetic())
        || !password.chars().any(|value| value.is_ascii_digit())
    {
        return Err(ApiError::bad_request(
            "Password must contain 12 to 256 characters, including a letter and a digit.",
        ));
    }
    Ok(())
}

fn verify_bootstrap_token(state: &AppState, supplied: &str) -> Result<(), ApiError> {
    let expected = state
        .config
        .admin_bootstrap_token
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Initial setup is not enabled."))?;
    if supplied.len() != expected.len()
        || !bool::from(supplied.as_bytes().ct_eq(expected.as_bytes()))
    {
        return Err(ApiError::unauthorized("The bootstrap token is invalid."));
    }
    Ok(())
}

async fn create_initial_super_admin(
    state: &AppState,
    display_name: &str,
    email: &str,
    password_hash: String,
) -> Result<StoredUser, ApiError> {
    let id = Uuid::new_v4();
    let normalized_email = email.to_ascii_lowercase();
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(672183921)")
            .execute(&mut *transaction)
            .await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
            .fetch_one(&mut *transaction)
            .await?;
        if exists {
            return Err(ApiError::conflict(
                "Initial setup is disabled after the first user is created.",
            ));
        }
        let role_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO roles (id, key, display_name, system_role)
               VALUES ($1, 'super-admin', 'Super Admin', true)
               ON CONFLICT (key) DO UPDATE SET display_name=EXCLUDED.display_name
               RETURNING id"#,
        )
        .bind(Uuid::new_v4())
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO role_permissions (role_id, permission_key) SELECT $1, key FROM permissions ON CONFLICT DO NOTHING",
        )
        .bind(role_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO users
               (id, email, password_hash, display_name, status, created_at, updated_at)
               VALUES ($1,$2,$3,$4,'active',now(),now())"#,
        )
        .bind(id)
        .bind(&normalized_email)
        .bind(&password_hash)
        .bind(display_name)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1,$2)")
            .bind(id)
            .bind(role_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
    } else {
        let mut data = state.data.write().await;
        if !data.admin_users.is_empty() {
            return Err(ApiError::conflict(
                "Initial setup is disabled after the first user is created.",
            ));
        }
        let user = StoredUser {
            id,
            display_name: display_name.into(),
            email: normalized_email.clone(),
            password_hash: password_hash.clone(),
            role: "Super Admin".into(),
            permissions: super_admin_permissions(),
            active: true,
            totp_enabled: false,
            totp_secret_ciphertext: None,
        };
        data.admin_users.insert(id, user);
    }
    Ok(StoredUser {
        id,
        display_name: display_name.into(),
        email: normalized_email,
        password_hash,
        role: "Super Admin".into(),
        permissions: super_admin_permissions(),
        active: true,
        totp_enabled: false,
        totp_secret_ciphertext: None,
    })
}

async fn find_user_by_email(state: &AppState, email: &str) -> Result<Option<StoredUser>, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "SELECT id, display_name, email, password_hash, status, totp_confirmed_at, totp_secret_ciphertext FROM users WHERE lower(email)=lower($1)",
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let id: Uuid = row.try_get("id")?;
        let (role, permissions) = load_roles_and_permissions(pool, id).await?;
        return Ok(Some(StoredUser {
            id,
            display_name: row.try_get("display_name")?,
            email: row.try_get("email")?,
            password_hash: row.try_get("password_hash")?,
            role,
            permissions,
            active: row.try_get::<String, _>("status")? == "active",
            totp_enabled: row
                .try_get::<Option<DateTime<Utc>>, _>("totp_confirmed_at")?
                .is_some(),
            totp_secret_ciphertext: row.try_get("totp_secret_ciphertext")?,
        }));
    }
    Ok(state
        .data
        .read()
        .await
        .admin_users
        .values()
        .find(|user| user.email.eq_ignore_ascii_case(email))
        .cloned())
}

async fn load_roles_and_permissions(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<(String, Vec<String>), ApiError> {
    let role_rows = sqlx::query(
        "SELECT role.display_name FROM roles AS role JOIN user_roles ON user_roles.role_id=role.id WHERE user_roles.user_id=$1 ORDER BY role.display_name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let role = role_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("display_name").ok())
        .collect::<Vec<_>>()
        .join(" · ");
    let permission_rows = sqlx::query(
        r#"SELECT DISTINCT role_permission.permission_key
           FROM role_permissions AS role_permission
           JOIN user_roles ON user_roles.role_id=role_permission.role_id
           WHERE user_roles.user_id=$1 ORDER BY role_permission.permission_key"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let permissions = permission_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("permission_key").ok())
        .collect();
    Ok((role, permissions))
}

async fn create_session(state: &AppState, user: StoredUser) -> Result<SessionIssue, ApiError> {
    let session_token = random_token();
    let csrf_token = random_token();
    let session_id = Uuid::new_v4();
    let session_token_hash = token_hash(&session_token);
    let csrf_hash = token_hash(&csrf_token);
    let expires_at = Utc::now() + Duration::hours(SESSION_HOURS);
    let created_at = Utc::now();
    if let Some(pool) = &state.pool {
        sqlx::query(
            r#"INSERT INTO sessions
               (id, user_id, token_hash, csrf_hash, created_at, expires_at, last_seen_at)
               VALUES ($1,$2,$3,$4,now(),$5,now())"#,
        )
        .bind(session_id)
        .bind(user.id)
        .bind(&session_token_hash)
        .bind(&csrf_hash)
        .bind(expires_at)
        .execute(pool)
        .await?;
        sqlx::query("UPDATE users SET last_login_at=now(), updated_at=now() WHERE id=$1")
            .bind(user.id)
            .execute(pool)
            .await?;
    } else {
        state.data.write().await.admin_sessions.insert(
            session_id,
            StoredSession {
                id: session_id,
                user_id: user.id,
                token_hash: session_token_hash.clone(),
                csrf_hash: csrf_hash.clone(),
                created_at,
                last_seen_at: created_at,
                expires_at,
                revoked: false,
            },
        );
    }
    Ok(SessionIssue {
        principal: AdminPrincipal {
            user_id: user.id,
            display_name: user.display_name,
            email: user.email,
            role: user.role,
            permissions: user.permissions,
            session_id,
            session_token_hash,
            csrf_hash,
            totp_enabled: user.totp_enabled,
        },
        session_token,
        csrf_token,
    })
}

async fn rotate_csrf(
    state: &AppState,
    principal: &AdminPrincipal,
    csrf_token: &str,
) -> Result<(), ApiError> {
    let csrf_hash = token_hash(csrf_token);
    if let Some(pool) = &state.pool {
        sqlx::query("UPDATE sessions SET csrf_hash=$1, last_seen_at=now() WHERE id=$2")
            .bind(&csrf_hash)
            .bind(principal.session_id)
            .execute(pool)
            .await?;
    } else if let Some(session) = state
        .data
        .write()
        .await
        .admin_sessions
        .get_mut(&principal.session_id)
    {
        session.csrf_hash = csrf_hash;
        session.last_seen_at = Utc::now();
    }
    Ok(())
}

async fn revoke_session(state: &AppState, principal: &AdminPrincipal) -> Result<(), ApiError> {
    if let Some(pool) = &state.pool {
        sqlx::query("UPDATE sessions SET revoked_at=now() WHERE id=$1")
            .bind(principal.session_id)
            .execute(pool)
            .await?;
    } else if let Some(session) = state
        .data
        .write()
        .await
        .admin_sessions
        .get_mut(&principal.session_id)
    {
        session.revoked = true;
    }
    Ok(())
}

fn session_response(state: &AppState, issue: SessionIssue, status: StatusCode) -> Response {
    let user = issue.principal.session_user(state);
    let mut response = (status, Json(user)).into_response();
    append_session_cookies(
        state,
        response.headers_mut(),
        &issue.session_token,
        &issue.csrf_token,
    );
    response.headers_mut().insert(
        CSRF_HEADER,
        HeaderValue::from_str(&issue.csrf_token).expect("generated CSRF token is a header value"),
    );
    append_no_store(response.headers_mut());
    response
}

fn session_refresh_response(
    state: &AppState,
    principal: AdminPrincipal,
    csrf_token: String,
) -> Response {
    let mut response = Json(principal.session_user(state)).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie(state, &csrf_token, SESSION_HOURS * 3600))
            .expect("generated CSRF cookie is valid"),
    );
    response.headers_mut().insert(
        CSRF_HEADER,
        HeaderValue::from_str(&csrf_token).expect("generated CSRF token is a header value"),
    );
    append_no_store(response.headers_mut());
    response
}

fn sensitive_json<T: Serialize>(value: T) -> Response {
    let mut response = Json(value).into_response();
    append_no_store(response.headers_mut());
    response
}

fn append_no_store(headers: &mut HeaderMap) {
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}

fn append_session_cookies(
    state: &AppState,
    headers: &mut HeaderMap,
    session_token: &str,
    csrf_token: &str,
) {
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(state, session_token, SESSION_HOURS * 3600))
            .expect("generated session cookie is valid"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie(state, csrf_token, SESSION_HOURS * 3600))
            .expect("generated CSRF cookie is valid"),
    );
}

fn append_clear_cookies(state: &AppState, headers: &mut HeaderMap) {
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(state, "", 0)).expect("clear cookie is valid"),
    );
    headers.append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie(state, "", 0)).expect("clear cookie is valid"),
    );
}

fn session_cookie(state: &AppState, value: &str, max_age: i64) -> String {
    format!(
        "{SESSION_COOKIE}={value}; Path=/api; Max-Age={max_age}; HttpOnly; SameSite=Strict{}",
        secure_attribute(state)
    )
}

fn csrf_cookie(state: &AppState, value: &str, max_age: i64) -> String {
    format!(
        "{CSRF_COOKIE}={value}; Path=/; Max-Age={max_age}; SameSite=Strict{}",
        secure_attribute(state)
    )
}

fn secure_attribute(state: &AppState) -> &'static str {
    if state.config.production || state.config.admin_origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    }
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then(|| value.to_owned()))
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|value| value.to_string())
        .map_err(|_| ApiError::internal("Password hashing failed."))
}

fn verify_password(encoded: &str, password: &str) -> bool {
    PasswordHash::new(encoded).ok().is_some_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}

fn token_hash(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

fn hex_digest(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn request_id(headers: &HeaderMap) -> Uuid {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

fn random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn request_source(
    state: &AppState,
    headers: &HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
) -> String {
    crate::rate_limit::resolve_client_source(
        headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        &state.config.trusted_proxy_cidrs,
    )
}

fn rate_limit_key(scope: &str, source: &str, account: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{scope}|{}|{}",
                source.trim(),
                account.trim().to_ascii_lowercase()
            )
            .as_bytes()
        )
    )
}

fn rate_limit_keys(scope: &str, source: &str, account: &str) -> Vec<String> {
    vec![
        rate_limit_key(scope, "all-sources", account),
        rate_limit_key(scope, source, account),
    ]
}

async fn check_auth_rate_limits(state: &AppState, keys: &[String]) -> Result<(), ApiError> {
    for key in keys {
        check_auth_rate_limit(state, key).await?;
    }
    Ok(())
}

async fn record_auth_failures(state: &AppState, keys: &[String]) -> Result<(), ApiError> {
    for key in keys {
        record_auth_failure(state, key).await?;
    }
    Ok(())
}

async fn clear_auth_rate_limits(state: &AppState, keys: &[String]) -> Result<(), ApiError> {
    for key in keys {
        clear_auth_rate_limit(state, key).await?;
    }
    Ok(())
}

async fn check_auth_rate_limit(state: &AppState, key: &str) -> Result<(), ApiError> {
    let now = Utc::now();
    if let Some(pool) = &state.pool {
        let blocked_until = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT blocked_until FROM auth_rate_limits WHERE key_hash=$1",
        )
        .bind(key)
        .fetch_optional(pool)
        .await?
        .flatten();
        if blocked_until.is_some_and(|until| until > now) {
            return Err(ApiError::too_many_requests(
                "Too many authentication attempts. Try again later.",
            ));
        }
        return Ok(());
    }
    if state
        .data
        .read()
        .await
        .auth_rate_limits
        .get(key)
        .and_then(|record| record.blocked_until)
        .is_some_and(|until| until > now)
    {
        return Err(ApiError::too_many_requests(
            "Too many authentication attempts. Try again later.",
        ));
    }
    Ok(())
}

async fn record_auth_failure(state: &AppState, key: &str) -> Result<(), ApiError> {
    let now = Utc::now();
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(key)
            .execute(&mut *transaction)
            .await?;
        let row = sqlx::query(
            "SELECT attempts, window_started_at FROM auth_rate_limits WHERE key_hash=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *transaction)
        .await?;
        let (attempts, window_started_at) = if let Some(row) = row {
            let started: DateTime<Utc> = row.try_get("window_started_at")?;
            let attempts: i32 = row.try_get("attempts")?;
            if started + Duration::minutes(RATE_WINDOW_MINUTES) <= now {
                (1_u32, now)
            } else {
                (attempts.max(0) as u32 + 1, started)
            }
        } else {
            (1, now)
        };
        let blocked_until =
            (attempts >= RATE_MAX_FAILURES).then(|| now + Duration::minutes(RATE_BLOCK_MINUTES));
        sqlx::query(
            r#"INSERT INTO auth_rate_limits
               (key_hash, attempts, window_started_at, blocked_until, updated_at)
               VALUES ($1,$2,$3,$4,$5)
               ON CONFLICT (key_hash) DO UPDATE SET attempts=EXCLUDED.attempts,
               window_started_at=EXCLUDED.window_started_at,
               blocked_until=EXCLUDED.blocked_until, updated_at=EXCLUDED.updated_at"#,
        )
        .bind(key)
        .bind(attempts as i32)
        .bind(window_started_at)
        .bind(blocked_until)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        return Ok(());
    }

    let mut data = state.data.write().await;
    data.auth_rate_limits.retain(|_, record| {
        record.updated_at + Duration::minutes(RATE_WINDOW_MINUTES + RATE_BLOCK_MINUTES) > now
    });
    if !data.auth_rate_limits.contains_key(key)
        && data.auth_rate_limits.len() >= RATE_MAX_MEMORY_KEYS
    {
        if let Some(oldest) = data
            .auth_rate_limits
            .iter()
            .min_by_key(|(_, record)| record.updated_at)
            .map(|(key, _)| key.clone())
        {
            data.auth_rate_limits.remove(&oldest);
        }
    }
    let record = data
        .auth_rate_limits
        .entry(key.to_owned())
        .or_insert(AuthRateLimit {
            attempts: 0,
            window_started_at: now,
            blocked_until: None,
            updated_at: now,
        });
    if record.window_started_at + Duration::minutes(RATE_WINDOW_MINUTES) <= now {
        record.attempts = 0;
        record.window_started_at = now;
        record.blocked_until = None;
    }
    record.attempts += 1;
    record.updated_at = now;
    if record.attempts >= RATE_MAX_FAILURES {
        record.blocked_until = Some(now + Duration::minutes(RATE_BLOCK_MINUTES));
    }
    Ok(())
}

async fn clear_auth_rate_limit(state: &AppState, key: &str) -> Result<(), ApiError> {
    if let Some(pool) = &state.pool {
        sqlx::query("DELETE FROM auth_rate_limits WHERE key_hash=$1")
            .bind(key)
            .execute(pool)
            .await?;
    } else {
        state.data.write().await.auth_rate_limits.remove(key);
    }
    Ok(())
}

fn valid_email(value: &str) -> bool {
    value.len() <= 254
        && !value.contains(':')
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
}

fn super_admin_permissions() -> Vec<String> {
    #[allow(unused_mut)]
    let mut values = [
        "dashboard.read",
        "content.read",
        "content.write",
        "content.publish",
        "product.read",
        "product.pricing.read",
        "product.write",
        "product.publish",
        "integration.run",
        "media.write",
        "rfq.read",
        "rfq.read_pii",
        "rfq.assign",
        "analytics.read",
        "identity.manage",
        "audit.read",
        "settings.manage",
        "operations.run",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    #[cfg(feature = "devtools")]
    values.push("devtools.shell".into());
    values
}

pub fn required_permission(path: &str, method: &axum::http::Method) -> Option<&'static str> {
    let write = !matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    );
    if (path.contains("/content/")
        || path.contains("/news/")
        || path.contains("/general-information/"))
        && (path.ends_with("/publish") || path.ends_with("/rollback"))
    {
        Some("content.publish")
    } else if path.ends_with("/content")
        || path.contains("/content/")
        || path.ends_with("/news")
        || path.contains("/news/")
        || path.contains("/general-information")
    {
        Some(if write {
            "content.write"
        } else {
            "content.read"
        })
    } else if path.contains("/products/") && path.ends_with("/private-pricing") {
        Some("product.pricing.read")
    } else if path.contains("/products/") && path.ends_with("/publish") {
        Some("product.publish")
    } else if path.ends_with("/products") || path.contains("/products/") {
        Some(if write {
            "product.write"
        } else {
            "product.read"
        })
    } else if path.contains("/feishu/") {
        Some("integration.run")
    } else if path.contains("/rfqs") || path.contains("/contacts") {
        Some("rfq.read")
    } else if path.contains("/analytics") {
        Some("analytics.read")
    } else if path.ends_with("/users")
        || path.contains("/users/")
        || path.ends_with("/roles")
        || path.contains("/roles/")
        || path.contains("/user-invitations")
    {
        Some("identity.manage")
    } else if path.ends_with("/settings") {
        Some("settings.manage")
    } else if path.contains("/operations") {
        Some("operations.run")
    } else if path.contains("/audit") {
        Some("audit.read")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn password_hashes_use_argon2id() {
        let hash = hash_password("a-long-password-123").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(&hash, "a-long-password-123"));
        assert!(!verify_password(&hash, "incorrect-password"));
    }

    #[test]
    fn invitation_password_policy_is_bounded_and_requires_letters_and_digits() {
        assert!(validate_strong_password("correct-horse-123").is_ok());
        assert!(validate_strong_password("short-1").is_err());
        assert!(validate_strong_password("onlylettersforever").is_err());
        assert!(validate_strong_password("1234567890123456").is_err());
        assert!(validate_strong_password(&format!("A1{}", "x".repeat(255))).is_err());
    }

    #[test]
    fn permission_mapping_distinguishes_edit_and_publish() {
        assert_eq!(
            required_permission("/api/admin/v1/content/abc", &axum::http::Method::PATCH),
            Some("content.write")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/content/abc/publish",
                &axum::http::Method::POST
            ),
            Some("content.publish")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/content/abc/preview",
                &axum::http::Method::POST
            ),
            Some("content.write")
        );
        assert_eq!(
            required_permission("/api/admin/v1/settings", &axum::http::Method::GET),
            Some("settings.manage")
        );
        assert_eq!(
            required_permission("/api/admin/v1/settings", &axum::http::Method::PATCH),
            Some("settings.manage")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/roles/00000000-0000-0000-0000-000000000001",
                &axum::http::Method::PATCH
            ),
            Some("identity.manage")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/products/00000000-0000-0000-0000-000000000001/private-pricing",
                &axum::http::Method::GET
            ),
            Some("product.pricing.read")
        );
    }

    #[test]
    fn permissions_are_unique() {
        let values = super_admin_permissions();
        let set = values.iter().collect::<HashSet<_>>();
        assert_eq!(set.len(), values.len());
    }

    #[test]
    fn https_cookie_is_secure_strict_and_host_only() {
        let mut config = crate::Config::for_test();
        config.admin_origin = "https://admin.example.com".into();
        let state = AppState::new(config).unwrap();
        let session = session_cookie(&state, "token", 60);
        let csrf = csrf_cookie(&state, "csrf", 60);
        assert!(session.contains("HttpOnly"));
        assert!(session.contains("SameSite=Strict"));
        assert!(session.contains("Secure"));
        assert!(csrf.contains("SameSite=Strict"));
        assert!(csrf.contains("Secure"));
        assert!(!session.contains("Domain="));
        assert!(!csrf.contains("Domain="));
    }
}
