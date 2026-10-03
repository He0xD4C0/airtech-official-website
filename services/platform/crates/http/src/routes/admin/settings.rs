use super::*;

use airtek_runtime::services::integration_settings::{
    self, UpdateCaptchaSettings, UpdateMailSettings, UpdateSmsSettings,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct MailTestRequest {
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SmsTestRequest {
    phone: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CaptchaTestRequest {
    token: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IntegrationTestResult {
    delivered: bool,
    verified: bool,
}

fn json_body<T: serde::de::DeserializeOwned>(
    payload: Result<Json<T>, JsonRejection>,
    shape: &str,
) -> Result<T, ApiError> {
    payload
        .map(|Json(value)| value)
        .map_err(|_| ApiError::bad_request(format!("{shape} must use the documented JSON shape.")))
}

fn settings_reason(reason: &str) -> Result<(), ApiError> {
    if reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "reason must contain at least 10 characters and is written to the audit trail.",
        ));
    }
    Ok(())
}

async fn audit_settings(
    state: &AppState,
    headers: &HeaderMap,
    action: &str,
    entity_type: &str,
    after: serde_json::Value,
) {
    let event = AuditEvent {
        id: Uuid::new_v4(),
        actor: actor(headers),
        action: action.into(),
        entity_type: entity_type.into(),
        entity_id: None,
        before: None,
        after: Some(after),
        reason: None,
        current_version: None,
        request_id: headers
            .get("x-request-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(Uuid::new_v4),
        occurred_at: Utc::now(),
    };
    if let Err(error) = state.persist_audit(event).await {
        tracing::warn!(error = %error, "Integration settings audit failed");
    }
}

fn private_json<T: serde::Serialize>(status: StatusCode, value: &T) -> Response {
    let mut response = (status, Json(value)).into_response();
    add_private_no_store_headers(&mut response);
    response
}

pub(super) async fn get_mail_settings(State(state): State<AppState>) -> Result<Response, ApiError> {
    let settings = integration_settings::get_mail(&state).await?;
    Ok(private_json(StatusCode::OK, &settings))
}

pub(super) async fn update_mail_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdateMailSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let update = json_body(payload, "Email settings")?;
    settings_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let settings = integration_settings::update_mail(&state, &actor, expected, &update).await?;
    audit_settings(
        &state,
        &headers,
        "settings.mail.update",
        "mailSettings",
        json!({"host": settings.host, "port": settings.port, "protocol": settings.protocol, "revision": settings.revision}),
    )
    .await;
    Ok(private_json(StatusCode::OK, &settings))
}

pub(super) async fn test_mail_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<MailTestRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let input = json_body(payload, "Email test")?;
    let transport = integration_settings::load_mail(&state)
        .await?
        .ok_or_else(|| ApiError::service_unavailable("Email delivery is not configured."))?;
    let recipient = input.to.unwrap_or_else(|| actor(&headers));
    airtek_runtime::services::outbound::send_mail(
        &state.request_metrics,
        &transport,
        &recipient,
        "AIRTEKPOWER SMTP test",
        "This message confirms the administration email settings.",
    )
    .await?;
    Ok(private_json(
        StatusCode::OK,
        &IntegrationTestResult {
            delivered: true,
            verified: false,
        },
    ))
}

pub(super) async fn get_sms_settings(State(state): State<AppState>) -> Result<Response, ApiError> {
    let settings = integration_settings::get_sms(&state).await?;
    Ok(private_json(StatusCode::OK, &settings))
}

pub(super) async fn update_sms_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdateSmsSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let update = json_body(payload, "SMS settings")?;
    settings_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let settings = integration_settings::update_sms(&state, &actor, expected, &update).await?;
    audit_settings(
        &state,
        &headers,
        "settings.sms.update",
        "smsSettings",
        json!({"provider": settings.provider, "signName": settings.sign_name, "revision": settings.revision}),
    )
    .await;
    Ok(private_json(StatusCode::OK, &settings))
}

pub(super) async fn test_sms_settings(
    State(state): State<AppState>,
    payload: Result<Json<SmsTestRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let input = json_body(payload, "SMS test")?;
    let transport = integration_settings::load_sms(&state)
        .await?
        .ok_or_else(|| ApiError::service_unavailable("SMS delivery is not configured."))?;
    let code: String = (1..=6)
        .map(|_| {
            use ring::rand::{SecureRandom, SystemRandom};
            let mut byte = [0_u8; 1];
            let _ = SystemRandom::new().fill(&mut byte);
            char::from(b'0' + (byte[0] % 10))
        })
        .collect();
    airtek_runtime::services::outbound::send_sms(
        &state.request_metrics,
        &transport,
        input.phone.trim(),
        &code,
    )
    .await?;
    Ok(private_json(
        StatusCode::OK,
        &IntegrationTestResult {
            delivered: true,
            verified: false,
        },
    ))
}

pub(super) async fn get_captcha_settings(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let settings = integration_settings::get_captcha(&state).await?;
    Ok(private_json(StatusCode::OK, &settings))
}

pub(super) async fn update_captcha_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdateCaptchaSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let update = json_body(payload, "CAPTCHA settings")?;
    settings_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let settings = integration_settings::update_captcha(&state, &actor, expected, &update).await?;
    audit_settings(
        &state,
        &headers,
        "settings.captcha.update",
        "captchaSettings",
        json!({"provider": settings.provider, "siteKey": settings.site_key, "revision": settings.revision}),
    )
    .await;
    Ok(private_json(StatusCode::OK, &settings))
}

pub(super) async fn test_captcha_settings(
    State(state): State<AppState>,
    payload: Result<Json<CaptchaTestRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let input = json_body(payload, "CAPTCHA test")?;
    let transport = integration_settings::load_captcha(&state)
        .await?
        .ok_or_else(|| ApiError::service_unavailable("CAPTCHA verification is not configured."))?;
    let verified = airtek_runtime::services::outbound::verify_captcha(
        &state.request_metrics,
        &transport,
        &input.token,
        None,
    )
    .await?;
    Ok(private_json(
        StatusCode::OK,
        &IntegrationTestResult {
            delivered: false,
            verified,
        },
    ))
}

pub(super) async fn get_settings(State(state): State<AppState>) -> Result<Response, ApiError> {
    let settings = state.platform_settings().await?;
    let revision = settings.revision;
    let mut response = entity_response(StatusCode::OK, &settings, revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdatePlatformSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(update) = payload.map_err(|_| {
        ApiError::bad_request(
            "Settings must be a valid JSON object containing only the documented mutable fields and reason.",
        )
    })?;
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    let settings = state
        .update_platform_settings(expected_revision, &update, &actor, request_id)
        .await?;
    let revision = settings.revision;
    let mut response = entity_response(StatusCode::OK, &settings, revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn get_object_storage_settings(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let settings = airtek_runtime::services::object_storage_settings::get(&state).await?;
    let mut response = entity_response(StatusCode::OK, &settings, settings.revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn test_object_storage_settings(
    State(state): State<AppState>,
    payload: Result<Json<ObjectStorageSettingsInput>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = payload.map_err(|_| {
        ApiError::bad_request("Object storage settings must use the documented JSON shape.")
    })?;
    let result = airtek_runtime::services::object_storage_settings::test(&state, &input).await?;
    let mut response = (StatusCode::OK, Json(result)).into_response();
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn update_object_storage_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdateObjectStorageSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(update) = payload.map_err(|_| {
        ApiError::bad_request("Object storage settings must use the documented JSON shape.")
    })?;
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    let settings = airtek_runtime::services::object_storage_settings::update(
        &state,
        expected_revision,
        &update,
        &actor,
        request_id,
    )
    .await?;
    let mut response = entity_response(StatusCode::OK, &settings, settings.revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}
