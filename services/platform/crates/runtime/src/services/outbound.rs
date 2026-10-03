//! Outbound delivery adapters for email, SMS and CAPTCHA verification.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;

use crate::error::ApiError;
use crate::services::integration_settings::{CaptchaTransport, MailTransport, SmsTransport};
use crate::services::request_metrics::{CaptchaOutcome, DeliveryChannel, RequestMetrics};

/// The only SMS endpoint in production. Development and acceptance stacks may
/// redirect delivery to a local stub through `AIRTEK_SMS_ENDPOINT`, but only
/// when `AIRTEK_SMS_ALLOW_ENDPOINT_OVERRIDE=true` is set explicitly.
const ALIYUN_SMS_ENDPOINT: &str = "https://dysmsapi.aliyuncs.com/";

fn delivery_failure(message: &str) -> ApiError {
    ApiError::service_unavailable(message.to_owned())
}

pub async fn send_mail(
    metrics: &RequestMetrics,
    transport: &MailTransport,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), ApiError> {
    let result = deliver_mail(transport, to, subject, body).await;
    metrics.record_delivery(DeliveryChannel::Mail, result.is_ok());
    result
}

async fn deliver_mail(
    transport: &MailTransport,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), ApiError> {
    let from = if transport.from_name.trim().is_empty() {
        transport
            .from_address
            .parse()
            .map_err(|_| delivery_failure("The configured from address is invalid."))?
    } else {
        lettre::message::Mailbox::new(
            Some(transport.from_name.clone()),
            transport
                .from_address
                .parse()
                .map_err(|_| delivery_failure("The configured from address is invalid."))?,
        )
    };
    let message = Message::builder()
        .from(from)
        .to(to
            .parse()
            .map_err(|_| delivery_failure("The recipient address is invalid."))?)
        .subject(subject)
        .body(body.to_owned())
        .map_err(|_| delivery_failure("The email message could not be built."))?;
    let mut builder = if transport.protocol == "plain" {
        // Credential-free plaintext relay, used by local development stacks and
        // acceptance mail capture. `update_mail` refuses credentials here.
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&transport.host)
    } else if transport.protocol == "tls" {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&transport.host)
            .map_err(|_| delivery_failure("The SMTP transport could not be created."))?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&transport.host)
            .map_err(|_| delivery_failure("The SMTP transport could not be created."))?
    }
    .port(u16::try_from(transport.port).unwrap_or(587));
    if !transport.username.is_empty() {
        builder = builder.credentials(Credentials::new(
            transport.username.clone(),
            transport.password.clone(),
        ));
    }
    builder
        .build()
        .send(message)
        .await
        .map(|_| ())
        .map_err(|error| {
            tracing::warn!(error = %error, "Outbound email delivery failed");
            delivery_failure("The verification email could not be delivered.")
        })
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn nonce() -> Result<String, ApiError> {
    let mut raw = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut raw)
        .map_err(|_| ApiError::internal("Signature nonce generation failed."))?;
    Ok(raw.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Sends a verification code through the Aliyun SMS RPC API. The signed query
/// follows the documented HMAC-SHA1 canonicalisation.
pub async fn send_sms(
    metrics: &RequestMetrics,
    transport: &SmsTransport,
    phone: &str,
    code: &str,
) -> Result<(), ApiError> {
    let result = deliver_sms(transport, phone, code).await;
    metrics.record_delivery(DeliveryChannel::Sms, result.is_ok());
    result
}

pub(super) fn sms_endpoint(
    override_endpoint: Option<&str>,
    allow_override: Option<&str>,
) -> String {
    if allow_override != Some("true") {
        return ALIYUN_SMS_ENDPOINT.to_owned();
    }
    let Some(endpoint) = override_endpoint.map(str::trim) else {
        return ALIYUN_SMS_ENDPOINT.to_owned();
    };
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return ALIYUN_SMS_ENDPOINT.to_owned();
    }
    tracing::warn!(
        endpoint,
        "AIRTEK_SMS_ENDPOINT is active; verification codes are delivered to the override endpoint"
    );
    endpoint.to_owned()
}

fn resolved_sms_endpoint() -> String {
    sms_endpoint(
        std::env::var("AIRTEK_SMS_ENDPOINT").ok().as_deref(),
        std::env::var("AIRTEK_SMS_ALLOW_ENDPOINT_OVERRIDE")
            .ok()
            .as_deref(),
    )
}

async fn deliver_sms(transport: &SmsTransport, phone: &str, code: &str) -> Result<(), ApiError> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let parameters = [
        ("AccessKeyId", transport.access_key_id.clone()),
        ("Action", "SendSms".to_owned()),
        ("Format", "JSON".to_owned()),
        ("PhoneNumbers", phone.to_owned()),
        ("RegionId", transport.region.clone()),
        ("SignName", transport.sign_name.clone()),
        ("SignatureMethod", "HMAC-SHA1".to_owned()),
        ("SignatureNonce", nonce()?),
        ("SignatureVersion", "1.0".to_owned()),
        ("TemplateCode", transport.template_code.clone()),
        ("TemplateParam", format!("{{\"code\":\"{code}\"}}")),
        ("Timestamp", timestamp),
        ("Version", "2017-05-25".to_owned()),
    ];
    let mut encoded = parameters
        .iter()
        .map(|(key, value)| (percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>();
    encoded.sort();
    let canonical = encoded
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let string_to_sign = format!("POST&%2F&{}", percent_encode(&canonical));
    let key = hmac::Key::new(
        hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        format!("{}&", transport.access_key_secret).as_bytes(),
    );
    let signature = BASE64.encode(hmac::sign(&key, string_to_sign.as_bytes()).as_ref());
    let mut form: Vec<(String, String)> = parameters
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect();
    form.push(("Signature".to_owned(), signature));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| ApiError::internal("The SMS client could not be created."))?;
    let response = client
        .post(resolved_sms_endpoint())
        .form(&form)
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "Outbound SMS delivery failed");
            delivery_failure("The verification SMS could not be delivered.")
        })?;
    let status = response.status();
    let payload = response.text().await.unwrap_or_default();
    if !status.is_success() || !payload.contains("\"Code\":\"OK\"") {
        tracing::warn!(%status, "Outbound SMS provider rejected the request");
        return Err(delivery_failure(
            "The verification SMS could not be delivered.",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CaptchaResponse {
    #[serde(default)]
    success: bool,
}

/// Returns `Ok(true)` for a valid token, `Ok(false)` for an explicitly rejected
/// token, and `Err` when the provider could not be reached at all. Callers
/// decide the configured failure mode.
pub async fn verify_captcha(
    metrics: &RequestMetrics,
    transport: &CaptchaTransport,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, ApiError> {
    let result = verify_captcha_token(transport, token, remote_ip).await;
    metrics.record_captcha(match &result {
        Ok(true) => CaptchaOutcome::Passed,
        Ok(false) => CaptchaOutcome::Rejected,
        Err(_) => CaptchaOutcome::Degraded,
    });
    result
}

async fn verify_captcha_token(
    transport: &CaptchaTransport,
    token: &str,
    remote_ip: Option<&str>,
) -> Result<bool, ApiError> {
    let endpoint = match transport.provider.as_str() {
        "turnstile" => "https://challenges.cloudflare.com/turnstile/v0/siteverify",
        "recaptcha" => "https://www.google.com/recaptcha/api/siteverify",
        "hcaptcha" => "https://hcaptcha.com/siteverify",
        _ => {
            return Err(ApiError::service_unavailable(
                "The CAPTCHA provider is unsupported.",
            ))
        }
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| ApiError::internal("The CAPTCHA client could not be created."))?;
    let mut form = vec![
        ("secret", transport.secret_key.clone()),
        ("response", token.to_owned()),
    ];
    if let Some(ip) = remote_ip {
        form.push(("remoteip", ip.to_owned()));
    }
    let response = client
        .post(endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "CAPTCHA verification request failed");
            delivery_failure("The CAPTCHA provider could not be reached.")
        })?;
    let parsed: CaptchaResponse = response
        .json()
        .await
        .map_err(|_| delivery_failure("The CAPTCHA provider returned an invalid response."))?;
    Ok(parsed.success)
}

#[cfg(test)]
mod tests {
    use super::{sms_endpoint, ALIYUN_SMS_ENDPOINT};

    #[test]
    fn sms_endpoint_override_requires_the_explicit_flag_and_an_http_url() {
        assert_eq!(sms_endpoint(Some("http://stub:8099/sms"), None), ALIYUN_SMS_ENDPOINT);
        assert_eq!(
            sms_endpoint(Some("http://stub:8099/sms"), Some("false")),
            ALIYUN_SMS_ENDPOINT
        );
        assert_eq!(
            sms_endpoint(Some("stub:8099"), Some("true")),
            ALIYUN_SMS_ENDPOINT
        );
        assert_eq!(
            sms_endpoint(Some(" http://stub:8099/sms "), Some("true")),
            "http://stub:8099/sms"
        );
    }
}
