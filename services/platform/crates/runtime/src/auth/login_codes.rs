//! Primary-method verification codes for the multi-step sign-in flow.
//!
//! Unknown accounts and accounts without a verified phone return before any
//! delivery happens so the caller can keep the response identical.

use super::login_flow::six_digit_code;
use super::*;
use crate::services::{integration_settings, outbound};

const CODE_MINUTES: i64 = 10;
const RESEND_SECONDS: i64 = 60;

pub(super) async fn send_primary_code(
    state: &AppState,
    flow_id: Uuid,
    last_sent_at: Option<DateTime<Utc>>,
    user: Option<&StoredUser>,
    factor: &str,
) -> Result<(), ApiError> {
    let Some(user) = user else {
        // Unknown account: pretend a code was sent so existence is not leaked.
        tracing::info!(flow = %flow_id, "Sign-in code suppressed for an unknown account");
        return Ok(());
    };
    if !user.active {
        return Ok(());
    }
    if let Some(last) = last_sent_at {
        if last + Duration::seconds(RESEND_SECONDS) > Utc::now() {
            return Err(ApiError::too_many_requests(
                "A verification code was sent recently. Try again shortly.",
            ));
        }
    }
    let code = six_digit_code()?;
    let hash = hash_password(&code)?;
    let destination = if factor == "emailCode" {
        Some(user.email.clone())
    } else {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT phone_e164 FROM users WHERE id=$1 AND phone_verified_at IS NOT NULL",
        )
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?
        .flatten()
    };
    let Some(destination) = destination else {
        return Ok(());
    };
    if factor == "emailCode" {
        if let Some(transport) = integration_settings::load_mail(state).await? {
            outbound::send_mail(
                &state.request_metrics,
                &transport,
                &destination,
                "AIRTEKPOWER sign-in code",
                &format!("Your AIRTEKPOWER sign-in code is {code}. It expires in 10 minutes."),
            )
            .await?;
        }
    } else if let Some(transport) = integration_settings::load_sms(state).await? {
        outbound::send_sms(&state.request_metrics, &transport, &destination, &code).await?;
    }
    sqlx::query(
        r#"UPDATE auth_login_flows SET primary_method=$2, pending_factor=$2,
             pending_destination=$3, code_hash=$4,
             code_expires_at=now() + ($5::bigint * interval '1 minute'),
             code_attempts=0, send_count=send_count+1, last_sent_at=now()
           WHERE id=$1"#,
    )
    .bind(flow_id)
    .bind(factor)
    .bind(&destination)
    .bind(&hash)
    .bind(CODE_MINUTES)
    .execute(&state.pool)
    .await?;
    Ok(())
}
