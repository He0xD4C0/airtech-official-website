//! Google-style multi-step administrator sign-in.
//!
//! Order of factors is fixed: primary method -> risk phone verification ->
//! TOTP. Unknown accounts, and accounts without a verified phone, are handled
//! without revealing which case applies.

use super::*;
use crate::config::CaptchaFailureMode;
use crate::services::{integration_settings, outbound};

const FLOW_MINUTES: i64 = 15;
const CODE_MINUTES: i64 = 10;
const MAX_CODE_ATTEMPTS: i32 = 5;
const RISK_THRESHOLD: i32 = 10;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentifyRequest {
    email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttemptRequest {
    flow_token: String,
    method: String,
    password: Option<String>,
    captcha_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyRequest {
    flow_token: String,
    code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifyResponse {
    flow_token: String,
    captcha_required: bool,
    captcha_site_key: Option<String>,
    captcha_provider: Option<String>,
    methods: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginStepResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    factor: Option<String>,
}

struct FlowRow {
    id: Uuid,
    email: String,
    primary_method: Option<String>,
    pending_factor: Option<String>,
    code_hash: Option<String>,
    code_expires_at: Option<DateTime<Utc>>,
    code_attempts: i32,
    last_sent_at: Option<DateTime<Utc>>,
}

fn step(status: &'static str, factor: Option<&str>) -> Response {
    let mut response = (
        StatusCode::ACCEPTED,
        Json(LoginStepResponse {
            status,
            factor: factor.map(str::to_owned),
        }),
    )
        .into_response();
    append_no_store(response.headers_mut());
    response
}

pub(super) fn six_digit_code() -> Result<String, ApiError> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0_u8; 6];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ApiError::internal("Verification code generation failed."))?;
    Ok(bytes
        .iter()
        .map(|byte| char::from(b'0' + (byte % 10)))
        .collect())
}

async fn load_flow(state: &AppState, token: &str) -> Result<Option<FlowRow>, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,email,primary_method,pending_factor,code_hash,code_expires_at,
                  code_attempts,last_sent_at
           FROM auth_login_flows
           WHERE flow_token_hash=$1 AND status='pending' AND expires_at>now()"#,
    )
    .bind(token_hash(token))
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(FlowRow {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        primary_method: row.try_get("primary_method")?,
        pending_factor: row.try_get("pending_factor")?,
        code_hash: row.try_get("code_hash")?,
        code_expires_at: row.try_get("code_expires_at")?,
        code_attempts: row.try_get("code_attempts")?,
        last_sent_at: row.try_get("last_sent_at")?,
    }))
}

pub async fn identify(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<IdentifyRequest>,
) -> Result<Response, ApiError> {
    let email = request.email.trim().to_ascii_lowercase();
    if email.len() < 3 || email.len() > 254 || !email.contains('@') {
        return Err(ApiError::bad_request("email is invalid."));
    }
    let rate_keys = rate_limit_keys("identify", &request_source(&state, &headers, peer), &email);
    check_auth_rate_limits(&state, &rate_keys).await?;
    let user_id = find_user_by_email(&state, &email)
        .await?
        .map(|user| user.id);
    let token = random_token();
    sqlx::query(
        r#"INSERT INTO auth_login_flows
             (id, flow_token_hash, email, user_id, source_hash, status, expires_at)
           VALUES ($1,$2,$3,$4,$5,'pending', now() + ($6::bigint * interval '1 minute'))"#,
    )
    .bind(Uuid::new_v4())
    .bind(token_hash(&token))
    .bind(&email)
    .bind(user_id)
    .bind(rate_keys.first().cloned().unwrap_or_default())
    .bind(FLOW_MINUTES)
    .execute(&state.pool)
    .await?;
    let captcha = integration_settings::load_captcha(&state).await?;
    Ok(sensitive_json(IdentifyResponse {
        flow_token: token,
        captcha_required: captcha.is_some(),
        captcha_site_key: captcha.as_ref().map(|value| value.site_key.clone()),
        captcha_provider: captcha.as_ref().map(|value| value.provider.clone()),
        methods: vec!["password", "emailCode", "smsCode"],
    }))
}

/// Applies the configured CAPTCHA failure mode. Returns `Ok(true)` when the
/// caller may continue with the requested method and `Ok(false)` when the
/// fail-closed fallback must restrict sign-in to the password method.
///
/// When the provider cannot be reached at all, fail-open keeps every method
/// available; fail-closed keeps passwords only, so an outage never disables
/// the account entirely nor silently skips the check for the code methods.
pub(super) fn captcha_unavailable_allows(mode: CaptchaFailureMode, method: &str) -> bool {
    match mode {
        CaptchaFailureMode::FailOpen => true,
        CaptchaFailureMode::FailClosed => method == "password",
    }
}

async fn captcha_guard(
    state: &AppState,
    headers: &HeaderMap,
    request: &AttemptRequest,
    method: &str,
) -> Result<bool, ApiError> {
    let Some(transport) = integration_settings::load_captcha(state).await? else {
        return Ok(true);
    };
    let Some(token) = request
        .captcha_token
        .as_deref()
        .filter(|value| !value.is_empty())
    else {
        return Err(ApiError::forbidden("A CAPTCHA token is required."));
    };
    let remote_ip = request_source(state, headers, None);
    match outbound::verify_captcha(
        &state.request_metrics,
        &transport,
        token,
        Some(remote_ip.as_str()),
    )
    .await
    {
        Ok(true) => Ok(true),
        Ok(false) => Err(ApiError::forbidden(
            "The CAPTCHA challenge was not accepted.",
        )),
        Err(error) => {
            let allowed = captcha_unavailable_allows(state.config.captcha_failure_mode, method);
            tracing::warn!(
                error = %error,
                mode = ?state.config.captcha_failure_mode,
                method,
                allowed,
                "CAPTCHA provider unavailable; applying the configured failure mode"
            );
            Ok(allowed)
        }
    }
}

pub async fn attempt(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<AttemptRequest>,
) -> Result<Response, ApiError> {
    let Some(flow) = load_flow(&state, &request.flow_token).await? else {
        return Err(ApiError::unauthorized("The sign-in attempt expired."));
    };
    let method = request.method.as_str();
    if !matches!(method, "password" | "emailCode" | "smsCode") {
        return Err(ApiError::bad_request("method is invalid."));
    }
    if !captcha_guard(&state, &headers, &request, method).await? {
        return Err(ApiError::forbidden(
            "Only password sign-in is available while CAPTCHA verification is unavailable.",
        ));
    }
    let rate_keys = rate_limit_keys(
        "login",
        &request_source(&state, &headers, peer),
        &flow.email,
    );
    check_auth_rate_limits(&state, &rate_keys).await?;
    let user = find_user_by_email(&state, &flow.email).await?;
    match method {
        "password" => {
            let supplied = request.password.as_deref().unwrap_or_default();
            let valid = match &user {
                Some(user) => user.active && verify_password(&user.password_hash, supplied),
                None => {
                    let dummy = DUMMY_PASSWORD_HASH.get_or_init(|| {
                        hash_password("invalid-login-password")
                            .expect("the fixed Argon2 dummy password must be hashable")
                    });
                    let _ = verify_password(dummy, supplied);
                    false
                }
            };
            if !valid {
                record_login_failures(&state, &rate_keys).await?;
                return Err(ApiError::unauthorized("Email or password is incorrect."));
            }
            let user = user.expect("valid password has a user");
            let risk_required = risk_applies(&state, &rate_keys, &user).await?;
            sqlx::query(
                r#"UPDATE auth_login_flows SET primary_method='password', risk_required=$2
                   WHERE id=$1"#,
            )
            .bind(flow.id)
            .bind(risk_required)
            .execute(&state.pool)
            .await?;
            advance(&state, &flow, user, false, &rate_keys).await
        }
        "emailCode" | "smsCode" => {
            send_primary_code(&state, flow.id, flow.last_sent_at, user.as_ref(), method).await?;
            Ok(step("codeSent", Some(method)))
        }
        _ => unreachable!("method was validated"),
    }
}

async fn risk_applies(
    state: &AppState,
    rate_keys: &[String],
    user: &StoredUser,
) -> Result<bool, ApiError> {
    if user.role_keys.iter().any(|key| key == "super-admin") {
        return Ok(false);
    }
    let Some(key) = rate_keys.first() else {
        return Ok(false);
    };
    let attempts: Option<i32> =
        sqlx::query_scalar("SELECT attempts FROM auth_rate_limits WHERE key_hash=$1")
            .bind(key)
            .fetch_optional(&state.pool)
            .await?;
    Ok(attempts.unwrap_or(0) >= RISK_THRESHOLD)
}

pub async fn verify(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<VerifyRequest>,
) -> Result<Response, ApiError> {
    let Some(flow) = load_flow(&state, &request.flow_token).await? else {
        return Err(ApiError::unauthorized("The sign-in attempt expired."));
    };
    let Some(factor) = flow.pending_factor.clone() else {
        return Err(ApiError::conflict("No verification step is pending."));
    };
    let rate_keys = rate_limit_keys(
        "login",
        &request_source(&state, &headers, peer),
        &flow.email,
    );
    if factor == "totp" {
        let user = find_user_by_email(&state, &flow.email)
            .await?
            .ok_or_else(|| ApiError::unauthorized("The sign-in attempt expired."))?;
        let ciphertext = user.totp_secret_ciphertext.as_deref().ok_or_else(|| {
            ApiError::service_unavailable("The confirmed TOTP secret is unavailable.")
        })?;
        let valid = EnrollmentSecret::open(ciphertext, totp_encryption_key(&state)?, user.id)?
            .verify_current(request.code.trim(), &user.email)?;
        if !valid {
            return Err(ApiError::unauthorized(
                "The TOTP code is invalid or expired.",
            ));
        }
        return advance(&state, &flow, user, true, &rate_keys).await;
    }
    let Some(hash) = flow.code_hash.as_deref() else {
        return Err(ApiError::conflict("No verification code is pending."));
    };
    if flow
        .code_expires_at
        .is_none_or(|expires| expires <= Utc::now())
    {
        return Err(ApiError::unauthorized("The verification code expired."));
    }
    if flow.code_attempts >= MAX_CODE_ATTEMPTS {
        return Err(ApiError::too_many_requests(
            "Too many incorrect verification codes.",
        ));
    }
    if !verify_password(hash, request.code.trim()) {
        sqlx::query("UPDATE auth_login_flows SET code_attempts=code_attempts+1 WHERE id=$1")
            .bind(flow.id)
            .execute(&state.pool)
            .await?;
        record_login_failures(&state, &rate_keys).await?;
        return Err(ApiError::unauthorized(
            "The verification code is invalid or expired.",
        ));
    }
    let user = find_user_by_email(&state, &flow.email)
        .await?
        .ok_or_else(|| ApiError::unauthorized("The sign-in attempt expired."))?;
    if factor == "riskSms" {
        sqlx::query("UPDATE auth_login_flows SET risk_required=false WHERE id=$1")
            .bind(flow.id)
            .execute(&state.pool)
            .await?;
    }
    advance(&state, &flow, user, false, &rate_keys).await
}

/// Applies the remaining factors and, when none are outstanding, issues the
/// session. `risk_required` is stored on the flow so a risk code is never sent
/// twice for the same attempt.
async fn advance(
    state: &AppState,
    flow: &FlowRow,
    user: StoredUser,
    totp_satisfied: bool,
    rate_keys: &[String],
) -> Result<Response, ApiError> {
    let risk_required = if flow.primary_method.as_deref() == Some("smsCode") {
        false
    } else {
        sqlx::query_scalar::<_, bool>("SELECT risk_required FROM auth_login_flows WHERE id=$1")
            .bind(flow.id)
            .fetch_one(&state.pool)
            .await?
    };
    if risk_required && flow.pending_factor.as_deref() != Some("riskSms") {
        let Some(phone) = sqlx::query_scalar::<_, Option<String>>(
            "SELECT phone_e164 FROM users WHERE id=$1 AND phone_verified_at IS NOT NULL",
        )
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?
        .flatten() else {
            return Err(ApiError::forbidden(
                "This account must verify a phone number with an administrator before signing in.",
            ));
        };
        let code = six_digit_code()?;
        let hash = hash_password(&code)?;
        state.request_metrics.record_risk_challenge();
        if let Some(transport) = integration_settings::load_sms(state).await? {
            outbound::send_sms(&state.request_metrics, &transport, &phone, &code).await?;
        }
        sqlx::query(
            r#"UPDATE auth_login_flows SET pending_factor='riskSms', pending_destination=$2,
                 code_hash=$3, code_expires_at=now() + ($4::bigint * interval '1 minute'),
                 code_attempts=0, send_count=send_count+1, last_sent_at=now()
               WHERE id=$1"#,
        )
        .bind(flow.id)
        .bind(&phone)
        .bind(&hash)
        .bind(CODE_MINUTES)
        .execute(&state.pool)
        .await?;
        return Ok(step("factorRequired", Some("riskSms")));
    }
    if user.totp_enabled && !totp_satisfied {
        sqlx::query(
            "UPDATE auth_login_flows SET pending_factor='totp', code_hash=NULL WHERE id=$1",
        )
        .bind(flow.id)
        .execute(&state.pool)
        .await?;
        return Ok(step("factorRequired", Some("totp")));
    }
    sqlx::query(
        "UPDATE auth_login_flows SET status='authenticated', consumed_at=now() WHERE id=$1",
    )
    .bind(flow.id)
    .execute(&state.pool)
    .await?;
    let issue = create_session(state, user).await?;
    record_auth_audit(
        state,
        &issue.principal.email,
        "auth.login",
        issue.principal.user_id,
        json!({"secondFactor": "loginFlow"}),
        &HeaderMap::new(),
    )
    .await?;
    clear_auth_rate_limits(state, rate_keys).await?;
    Ok(session_response(state, issue, StatusCode::OK))
}
