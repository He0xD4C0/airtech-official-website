//! Self-service administrator phone binding.
//!
//! SMS sign-in and risk verification only trust a phone that the account holder
//! confirmed with a code delivered to that number, so the pending challenge
//! stays in `auth_phone_verifications` until the code matches.

use super::login_flow::six_digit_code;
use super::*;
use crate::services::{integration_settings, outbound};

const CODE_MINUTES: i64 = 10;
const MAX_CODE_ATTEMPTS: i32 = 5;
const RESEND_SECONDS: i64 = 60;
const MAX_SENDS: i32 = 5;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartPhoneVerificationRequest {
    pub phone: String,
    pub current_password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfirmPhoneVerificationRequest {
    pub code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneVerificationStarted {
    pub status: &'static str,
}

/// Accepts `+` followed by 8-15 digits, ignoring spaces and dashes so a number
/// copied from a contact card still binds. The stored value is E.164 only.
pub fn normalize_e164(input: &str) -> Result<String, ApiError> {
    let normalized: String = input
        .chars()
        .filter(|value| !value.is_whitespace() && *value != '-' && *value != '(' && *value != ')')
        .collect();
    let digits = normalized.strip_prefix('+');
    let valid = digits.is_some_and(|value| {
        (8..=15).contains(&value.len())
            && value.bytes().all(|byte| byte.is_ascii_digit())
            && !value.starts_with('0')
    });
    if !valid {
        return Err(ApiError::bad_request(
            "phone must be an E.164 number such as +8613800138000.",
        ));
    }
    Ok(normalized)
}

fn masked(phone: &str) -> String {
    let digits = phone.trim_start_matches('+');
    match digits.len() {
        0..=4 => "****".to_owned(),
        _ => format!("+***{}", &digits[digits.len() - 4..]),
    }
}

async fn current_password_matches(
    state: &AppState,
    user_id: Uuid,
    supplied: &str,
) -> Result<bool, ApiError> {
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(verify_password(&hash, supplied))
}

async fn phone_taken(state: &AppState, phone: &str, user_id: Uuid) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE phone_e164=$1 AND id<>$2)",
    )
    .bind(phone)
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?)
}

/// Sends a binding code to the requested number. The current password is
/// required so a stolen session alone cannot redirect SMS recovery.
pub async fn start_phone_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StartPhoneVerificationRequest>,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    let phone = normalize_e164(&request.phone)?;
    if !current_password_matches(&state, principal.user_id, &request.current_password).await? {
        return Err(ApiError::unauthorized("The current password is incorrect."));
    }
    if phone_taken(&state, &phone, principal.user_id).await? {
        return Err(ApiError::conflict(
            "This phone number is already bound to another administrator.",
        ));
    }
    let Some(transport) = integration_settings::load_sms(&state).await? else {
        return Err(ApiError::service_unavailable(
            "SMS delivery is not configured; ask a Super Admin to configure it first.",
        ));
    };
    let existing = sqlx::query(
        "SELECT phone_e164, last_sent_at, send_count FROM auth_phone_verifications WHERE user_id=$1",
    )
    .bind(principal.user_id)
    .fetch_optional(&state.pool)
    .await?;
    let mut send_count = 0_i32;
    if let Some(row) = &existing {
        let previous_phone: String = row.try_get("phone_e164")?;
        let last_sent_at: DateTime<Utc> = row.try_get("last_sent_at")?;
        send_count = row.try_get("send_count")?;
        if previous_phone == phone && last_sent_at + Duration::seconds(RESEND_SECONDS) > Utc::now()
        {
            return Err(ApiError::too_many_requests(
                "A verification code was sent recently. Try again shortly.",
            ));
        }
        if previous_phone == phone && send_count >= MAX_SENDS {
            return Err(ApiError::too_many_requests(
                "Too many verification codes were sent. Contact a Super Admin to bind this number.",
            ));
        }
        if previous_phone != phone {
            send_count = 0;
        }
    }
    let code = six_digit_code()?;
    let hash = hash_password(&code)?;
    outbound::send_sms(&state.request_metrics, &transport, &phone, &code).await?;
    sqlx::query(
        r#"INSERT INTO auth_phone_verifications
             (user_id, phone_e164, code_hash, code_expires_at, code_attempts, send_count, last_sent_at)
           VALUES ($1,$2,$3, now() + ($4::bigint * interval '1 minute'), 0, 1, now())
           ON CONFLICT (user_id) DO UPDATE SET
             phone_e164=EXCLUDED.phone_e164, code_hash=EXCLUDED.code_hash,
             code_expires_at=EXCLUDED.code_expires_at, code_attempts=0,
             send_count=$5, last_sent_at=now()"#,
    )
    .bind(principal.user_id)
    .bind(&phone)
    .bind(&hash)
    .bind(CODE_MINUTES)
    .bind(send_count + 1)
    .execute(&state.pool)
    .await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.phone.verification_started",
        principal.user_id,
        json!({"phone": masked(&phone)}),
        &headers,
    )
    .await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(PhoneVerificationStarted { status: "codeSent" }),
    )
        .into_response())
}

/// Confirms the code and copies the number onto the account. A concurrent
/// binding for the same number loses the unique-index race and is reported as
/// a conflict instead of a server error.
pub async fn confirm_phone_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConfirmPhoneVerificationRequest>,
) -> Result<Response, ApiError> {
    let principal = authenticate_with_csrf(&state, &headers).await?;
    let row = sqlx::query(
        "SELECT phone_e164, code_hash, code_expires_at, code_attempts
         FROM auth_phone_verifications WHERE user_id=$1",
    )
    .bind(principal.user_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = row else {
        return Err(ApiError::conflict("No phone verification is pending."));
    };
    let phone: String = row.try_get("phone_e164")?;
    let code_hash: String = row.try_get("code_hash")?;
    let expires_at: DateTime<Utc> = row.try_get("code_expires_at")?;
    let attempts: i32 = row.try_get("code_attempts")?;
    if expires_at <= Utc::now() {
        return Err(ApiError::unauthorized("The verification code expired."));
    }
    if attempts >= MAX_CODE_ATTEMPTS {
        return Err(ApiError::too_many_requests(
            "Too many incorrect verification codes.",
        ));
    }
    if !verify_password(&code_hash, request.code.trim()) {
        sqlx::query(
            "UPDATE auth_phone_verifications SET code_attempts=code_attempts+1 WHERE user_id=$1",
        )
        .bind(principal.user_id)
        .execute(&state.pool)
        .await?;
        return Err(ApiError::unauthorized(
            "The verification code is invalid or expired.",
        ));
    }
    let mut transaction = state.pool.begin().await?;
    let conflict: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE phone_e164=$1 AND id<>$2)")
            .bind(&phone)
            .bind(principal.user_id)
            .fetch_one(&mut *transaction)
            .await?;
    if conflict {
        return Err(ApiError::conflict(
            "This phone number is already bound to another administrator.",
        ));
    }
    sqlx::query(
        "UPDATE users SET phone_e164=$2, phone_verified_at=now(), updated_at=now() WHERE id=$1",
    )
    .bind(principal.user_id)
    .bind(&phone)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("DELETE FROM auth_phone_verifications WHERE user_id=$1")
        .bind(principal.user_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    record_auth_audit(
        &state,
        &principal.email,
        "auth.phone.bound",
        principal.user_id,
        json!({"phone": masked(&phone)}),
        &headers,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::normalize_e164;

    #[test]
    fn normalized_numbers_are_compact_e164_only() {
        assert_eq!(
            normalize_e164(" +86 138-0013-8000 ").unwrap(),
            "+8613800138000"
        );
        assert!(normalize_e164("13800138000").is_err());
        assert!(normalize_e164("+0123456789").is_err());
        assert!(normalize_e164("+86138001380001234").is_err());
    }
}
