use std::{collections::BTreeMap, net::SocketAddr};

use axum::{
    extract::{connect_info::ConnectInfo, Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::ApiError,
    idempotency::{begin as begin_idempotency, IdempotencyOutcome},
    models::{
        AcceptedResponse, AnalyticsConsentReceipt, AnalyticsEventReceipt, ContactRequest,
        ContentKind, ContentPreviewResponse, CreateAnalyticsConsent, CreateAnalyticsEvent,
        CreateContactRequest, CreateRfqRequest, CursorPage, Product, ProductFamily, ProductQuery,
        RfqJourney, RfqSubmission, SelectorRequest, SelectorResponse,
    },
    preview_token::PreviewTokenError,
    rate_limit::{
        enforce_public_rate_limit, ANALYTICS_CONSENT_POLICY, ANALYTICS_POLICY, CONTACT_POLICY,
        RFQ_POLICY,
    },
    routes::etag,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/content-preview", get(get_content_preview))
        .route("/content/{kind}/{slug}", get(get_content))
        .route("/products", get(list_products))
        .route("/products/{slug}", get(get_product))
        .route("/discovery", get(discovery))
        .route("/selector", post(select_products))
        .route("/contact", post(create_contact))
        .route("/rfqs", post(create_rfq))
        .route("/analytics/consents", post(create_analytics_consent))
        .route("/analytics/events", post(create_analytics_event))
}

pub const ANALYTICS_POLICY_VERSION: &str = "analytics-v1";
const ANALYTICS_CONSENT_LIFETIME_DAYS: i64 = 180;

async fn get_content_preview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let result = load_content_preview(&state, &headers).await;
    let mut response = match result {
        Ok(preview) => (StatusCode::OK, Json(preview)).into_response(),
        Err(error) => error.into_response(),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow, noarchive"),
    );
    response
}

async fn load_content_preview(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ContentPreviewResponse, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(content_preview_not_found)?;
    if token.len() > crate::preview_token::MAX_PREVIEW_TOKEN_LENGTH {
        return Err(content_preview_not_found());
    }
    let key = state
        .config
        .preview_signing_key
        .as_ref()
        .ok_or_else(content_preview_not_found)?;
    let verified = match crate::preview_token::verify(key, token) {
        Ok(verified) => verified,
        Err(PreviewTokenError::Expired) => {
            return Err(ApiError::gone("Content preview has expired."));
        }
        Err(PreviewTokenError::Invalid) => return Err(content_preview_not_found()),
    };
    let content = state
        .load_content_revision(verified.content_id, verified.revision)
        .await?
        .filter(|content| {
            content.id == verified.content_id && content.current_revision == verified.revision
        })
        .ok_or_else(content_preview_not_found)?;
    Ok(ContentPreviewResponse {
        content,
        preview_expires_at: verified.expires_at,
    })
}

fn content_preview_not_found() -> ApiError {
    ApiError::not_found("Content preview was not found.")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryDocument {
    generated_at: chrono::DateTime<Utc>,
    entries: Vec<DiscoveryEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryEntry {
    entity_type: &'static str,
    entity_id: Uuid,
    path: String,
    locale: String,
    title: String,
    summary: Option<String>,
    updated_at: chrono::DateTime<Utc>,
}

async fn discovery(State(state): State<AppState>) -> Json<DiscoveryDocument> {
    let data = state.data.read().await;
    let mut entries = Vec::new();
    entries.extend(data.published_content.values().filter_map(|content| {
        let path = content.seo.canonical_path.as_ref()?;
        if content.is_placeholder || !content.seo.indexable || !valid_public_path(path) {
            return None;
        }
        Some(DiscoveryEntry {
            entity_type: "content",
            entity_id: content.id,
            path: path.clone(),
            locale: content.locale.clone(),
            title: content.title.clone(),
            summary: content.summary.clone(),
            updated_at: content.updated_at,
        })
    }));
    entries.extend(data.published_products.values().filter_map(|product| {
        if !product.indexable || product.locale != "en" || !valid_slug_segment(&product.slug) {
            return None;
        }
        Some(DiscoveryEntry {
            entity_type: "product",
            entity_id: product.id,
            path: format!(
                "/en/products/{}/{}",
                product_family_segment(product.family),
                product.slug
            ),
            locale: product.locale.clone(),
            title: product.title.clone(),
            summary: product.summary.clone(),
            updated_at: product.updated_at,
        })
    }));
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Json(DiscoveryDocument {
        generated_at: Utc::now(),
        entries,
    })
}

fn valid_public_path(path: &str) -> bool {
    (path == "/en" || path.starts_with("/en/"))
        && !path.contains('?')
        && !path.contains('#')
        && !path.contains("//")
        && !path.starts_with("/en/admin")
}

fn valid_slug_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn product_family_segment(value: crate::models::ProductFamily) -> &'static str {
    match value {
        crate::models::ProductFamily::Centrifugal => "centrifugal",
        crate::models::ProductFamily::Axial => "axial",
        crate::models::ProductFamily::CrossFlow => "cross-flow",
        crate::models::ProductFamily::InlineDuct => "inline-duct",
        crate::models::ProductFamily::Motors => "motors",
    }
}

#[derive(Debug, Deserialize)]
struct ContentQuery {
    #[serde(default = "default_locale")]
    locale: String,
}

async fn get_content(
    State(state): State<AppState>,
    Path((kind, slug)): Path<(String, String)>,
    Query(query): Query<ContentQuery>,
) -> Result<Response, ApiError> {
    let kind = parse_content_kind(&kind)?;
    let data = state.data.read().await;
    let entry = data
        .published_content
        .values()
        .find(|entry| entry.kind == kind && entry.slug == slug && entry.locale == query.locale)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Published content was not found."))?;
    let revision = entry.published_revision.unwrap_or(entry.current_revision);
    Ok(with_etag(entry, revision))
}

async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<ProductQuery>,
) -> Result<Json<CursorPage<Product>>, ApiError> {
    let limit = query.limit.unwrap_or(24);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let after = query
        .cursor
        .as_deref()
        .map(|value| decode_product_cursor(value, &query))
        .transpose()?;
    let mut products: Vec<_> = state
        .data
        .read()
        .await
        .published_products
        .values()
        .filter(|product| {
            query
                .family
                .map(|family| family == product.family)
                .unwrap_or(true)
                && query
                    .motor_technology
                    .as_ref()
                    .map(|technology| product.motor_technology.as_ref() == Some(technology))
                    .unwrap_or(true)
        })
        .cloned()
        .collect();
    products.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    if let Some(after) = after {
        products.retain(|product| product.stable_id > after);
    }
    let has_more = products.len() > limit;
    products.truncate(limit);
    let next_cursor = has_more
        .then(|| products.last())
        .flatten()
        .map(|product| encode_product_cursor(&product.stable_id, &query))
        .transpose()?;
    Ok(Json(CursorPage {
        items: products,
        next_cursor,
    }))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductCursor {
    version: u8,
    stable_id: String,
    family: Option<ProductFamily>,
    motor_technology: Option<String>,
}

fn encode_product_cursor(stable_id: &str, query: &ProductQuery) -> Result<String, ApiError> {
    let cursor = ProductCursor {
        version: 1,
        stable_id: stable_id.to_owned(),
        family: query.family,
        motor_technology: query.motor_technology.clone(),
    };
    serde_json::to_vec(&cursor)
        .map(|value| URL_SAFE_NO_PAD.encode(value))
        .map_err(|_| ApiError::internal("Product cursor serialization failed."))
}

fn decode_product_cursor(value: &str, query: &ProductQuery) -> Result<String, ApiError> {
    if value.is_empty() || value.len() > 2_048 {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<ProductCursor>(&value).ok())
        .filter(|cursor| {
            cursor.version == 1
                && !cursor.stable_id.is_empty()
                && cursor.stable_id.len() <= 500
                && cursor.family == query.family
                && cursor.motor_technology == query.motor_technology
        })
        .ok_or_else(|| {
            ApiError::bad_request("cursor is invalid or belongs to different product filters.")
        })?;
    Ok(decoded.stable_id)
}

async fn get_product(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, ApiError> {
    let data = state.data.read().await;
    let product = data
        .published_products
        .values()
        .find(|product| product.slug == slug)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Published product was not found."))?;
    let revision = product
        .published_revision
        .unwrap_or(product.current_revision);
    Ok(with_etag(product, revision))
}

async fn select_products(
    State(state): State<AppState>,
    Json(request): Json<SelectorRequest>,
) -> Result<Json<SelectorResponse>, ApiError> {
    validate_selector(&request)?;
    let products: Vec<Product> = state
        .data
        .read()
        .await
        .published_products
        .values()
        .filter(|product| {
            request
                .preferred_family
                .map(|family| family == product.family)
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    Ok(Json(crate::services::selector::evaluate(
        &request, &products,
    )))
}

async fn create_rfq(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<CreateRfqRequest>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        RFQ_POLICY,
    )
    .await?;
    let idempotency = match begin_idempotency(&state, "public.rfq", &headers, &request).await? {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let response: AcceptedResponse = replay.decode()?;
            return Ok((status, Json(response)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_rfq(&request)?;
    validate_product_rfq_context(&state, &request).await?;
    let retention_days = state
        .integer_setting("rfqRetentionDays", 365, 30, 3_650)
        .await?;

    let submitted_at = Utc::now();
    let id = Uuid::new_v4();
    let submission = RfqSubmission {
        id,
        reference: reference("RFQ", id, submitted_at),
        request,
        status: "new".into(),
        submitted_at,
        retention_until: submitted_at + Duration::days(retention_days),
    };
    state.persist_rfq(&submission).await?;
    state.data.write().await.rfqs.insert(id, submission.clone());
    let accepted = AcceptedResponse {
        id,
        reference: submission.reference,
        accepted_at: submitted_at,
    };
    idempotency
        .complete(&state, &accepted, StatusCode::CREATED)
        .await?;
    Ok((StatusCode::CREATED, Json(accepted)).into_response())
}

async fn validate_product_rfq_context(
    state: &AppState,
    request: &CreateRfqRequest,
) -> Result<(), ApiError> {
    if request.journey != RfqJourney::Product {
        return Ok(());
    }
    let context = request
        .product_context
        .as_ref()
        .expect("structural RFQ validation requires product context");
    let data = state.data.read().await;
    let published = data
        .published_products
        .get(&context.product_id)
        .ok_or_else(|| {
            ApiError::conflict(
                "The referenced product is not currently published; use Selection RFQ instead.",
            )
        })?;
    if published.stable_id != context.stable_id
        || published.model != context.model
        || published.published_revision != Some(context.published_revision)
    {
        return Err(ApiError::conflict(
            "Product RFQ context does not match the current immutable published product revision.",
        ));
    }
    Ok(())
}

async fn create_contact(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<CreateContactRequest>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        CONTACT_POLICY,
    )
    .await?;
    let idempotency = match begin_idempotency(&state, "public.contact", &headers, &request).await? {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let response: AcceptedResponse = replay.decode()?;
            return Ok((status, Json(response)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_contact(&request)?;
    let retention_days = state
        .integer_setting("rfqRetentionDays", 365, 30, 3_650)
        .await?;

    let submitted_at = Utc::now();
    let id = Uuid::new_v4();
    let contact = ContactRequest {
        id,
        reference: reference("CONTACT", id, submitted_at),
        request,
        status: "new".into(),
        submitted_at,
        retention_until: submitted_at + Duration::days(retention_days),
    };
    state.persist_contact(&contact).await?;
    state
        .data
        .write()
        .await
        .contacts
        .insert(id, contact.clone());
    let accepted = AcceptedResponse {
        id,
        reference: contact.reference,
        accepted_at: submitted_at,
    };
    idempotency
        .complete(&state, &accepted, StatusCode::CREATED)
        .await?;
    Ok((StatusCode::CREATED, Json(accepted)).into_response())
}

async fn create_analytics_consent(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<CreateAnalyticsConsent>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        ANALYTICS_CONSENT_POLICY,
    )
    .await?;
    if request.policy_version != ANALYTICS_POLICY_VERSION {
        return Err(ApiError::validation(BTreeMap::from([(
            "policyVersion".into(),
            vec!["The analytics policy version is not current.".into()],
        )])));
    }

    let granted_at = Utc::now();
    let receipt = AnalyticsConsentReceipt {
        consent_receipt: Uuid::new_v4(),
        anonymous_session_id: request.anonymous_session_id,
        policy_version: request.policy_version,
        analytics_allowed: request.analytics_allowed,
        granted_at,
        expires_at: granted_at + Duration::days(ANALYTICS_CONSENT_LIFETIME_DAYS),
    };
    state.persist_analytics_consent(&receipt).await?;
    state
        .data
        .write()
        .await
        .analytics_consents
        .insert(receipt.consent_receipt, receipt.clone());

    let mut response = (StatusCode::CREATED, Json(receipt)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}

async fn create_analytics_event(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(event): Json<CreateAnalyticsEvent>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        ANALYTICS_POLICY,
    )
    .await?;
    if !event.consent_granted {
        return Ok((
            StatusCode::ACCEPTED,
            Json(AnalyticsEventReceipt {
                accepted: false,
                event_id: None,
            }),
        )
            .into_response());
    }
    validate_analytics_event(&event)?;
    validate_analytics_consent(&state, &event).await?;
    let event_id = Uuid::new_v4();
    let event_value = serde_json::to_value(&event)
        .map_err(|_| ApiError::internal("Analytics serialization failed."))?;
    state
        .persist_analytics_event(event_id, &event_value)
        .await?;
    let receipt = AnalyticsEventReceipt {
        accepted: true,
        event_id: Some(event_id),
    };
    state
        .data
        .write()
        .await
        .analytics_receipts
        .insert(event_id, receipt.clone());
    Ok((StatusCode::ACCEPTED, Json(receipt)).into_response())
}

async fn validate_analytics_consent(
    state: &AppState,
    event: &CreateAnalyticsEvent,
) -> Result<(), ApiError> {
    let Some(anonymous_session_id) = event.anonymous_session_id else {
        return Err(ApiError::validation(BTreeMap::from([(
            "anonymousSessionId".into(),
            vec!["A consented analytics event requires an anonymous session ID.".into()],
        )])));
    };
    let Some(policy_version) = event.policy_version.as_deref() else {
        return Err(ApiError::validation(BTreeMap::from([(
            "policyVersion".into(),
            vec!["A consented analytics event requires a policy version.".into()],
        )])));
    };
    let Some(consent_receipt) = event.consent_receipt else {
        return Err(ApiError::validation(BTreeMap::from([(
            "consentReceipt".into(),
            vec!["A consented analytics event requires a consent receipt.".into()],
        )])));
    };
    let receipt = state
        .current_analytics_consent(consent_receipt)
        .await?
        .filter(|receipt| {
            receipt.analytics_allowed
                && receipt.expires_at > Utc::now()
                && receipt.anonymous_session_id == anonymous_session_id
                && receipt.policy_version == policy_version
                && policy_version == ANALYTICS_POLICY_VERSION
        });
    if receipt.is_none() {
        return Err(ApiError::validation(BTreeMap::from([(
            "consentReceipt".into(),
            vec!["The consent receipt is invalid, expired, denied, superseded, or does not match this session and policy.".into()],
        )])));
    }
    Ok(())
}

fn with_etag<T: serde::Serialize>(value: T, revision: i64) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response
}

fn parse_content_kind(value: &str) -> Result<ContentKind, ApiError> {
    match value {
        "home" => Ok(ContentKind::Home),
        "solutions" => Ok(ContentKind::Solution),
        "technology" => Ok(ContentKind::Technology),
        "articles" => Ok(ContentKind::Article),
        "faqs" => Ok(ContentKind::Faq),
        "case-studies" => Ok(ContentKind::CaseStudy),
        "downloads" => Ok(ContentKind::Download),
        "company" => Ok(ContentKind::Company),
        "legal" => Ok(ContentKind::Legal),
        _ => Err(ApiError::not_found("Content kind was not found.")),
    }
}

fn validate_selector(request: &SelectorRequest) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if !request.airflow.is_finite() || request.airflow <= 0.0 {
        errors.insert(
            "airflow".into(),
            vec!["Must be a positive finite value.".into()],
        );
    }
    if !request.pressure.is_finite() || request.pressure <= 0.0 {
        errors.insert(
            "pressure".into(),
            vec!["Must be a positive finite value.".into()],
        );
    }
    if !["m3/h", "m³/h", "cfm"].contains(&request.airflow_unit.to_lowercase().as_str()) {
        errors.insert(
            "airflowUnit".into(),
            vec!["Unsupported airflow unit.".into()],
        );
    }
    if !["pa", "kpa", "inh2o"].contains(&request.pressure_unit.to_lowercase().as_str()) {
        errors.insert(
            "pressureUnit".into(),
            vec!["Unsupported pressure unit.".into()],
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RfqQuantity {
    Integer(u64),
    Text(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RfqDutyPoint {
    airflow: f64,
    airflow_unit: String,
    pressure: f64,
    pressure_unit: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RfqElectricalContext {
    voltage: Option<String>,
    frequency_hz: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductRfqContext {
    application: String,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectionRfqContext {
    application: String,
    duty_point: RfqDutyPoint,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
    maximum_diameter_mm: Option<f64>,
    #[serde(default)]
    required_certifications: Vec<String>,
    control: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectRfqContext {
    application: String,
    project_stage: String,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
    project_scale: Option<String>,
    schedule: Option<String>,
    engineering_needs: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplacementRfqContext {
    application: String,
    existing_model: String,
    duty_point: RfqDutyPoint,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
    installation_constraints: Option<String>,
    replacement_goal: Option<String>,
}

fn validate_rfq(request: &CreateRfqRequest) -> Result<(), ApiError> {
    let mut errors = validate_business_contact(&request.contact);
    if !request.consent {
        errors.insert("consent".into(), vec!["Consent is required.".into()]);
    }
    validate_public_source_and_locale(
        &request.source_path,
        &request.locale,
        "/en/request-a-quote/",
        &mut errors,
    );

    let context_value = Value::Object(request.context.clone().into_iter().collect());
    if contains_attachment_field(&context_value) {
        errors.insert(
            "context".into(),
            vec!["Public RFQ attachments and file metadata are not accepted.".into()],
        );
    }

    match request.journey {
        RfqJourney::Product => {
            let product_context = match request.product_context.as_ref() {
                Some(context) => Some(context),
                None => {
                    errors.insert(
                        "productContext".into(),
                        vec!["Product RFQ requires an immutable published product context.".into()],
                    );
                    None
                }
            };
            if let Some(context) = product_context {
                validate_product_context(context, &mut errors);
            }
            if let Some(context) =
                parse_rfq_context::<ProductRfqContext>(context_value, "product", &mut errors)
            {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
            }
        }
        RfqJourney::Selection => {
            reject_product_context(request, &mut errors);
            if let Some(context) =
                parse_rfq_context::<SelectionRfqContext>(context_value, "selection", &mut errors)
            {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
                validate_duty_point(&context.duty_point, &mut errors);
                validate_optional_finite_positive(
                    "context.maximumDiameterMm",
                    context.maximum_diameter_mm,
                    100_000.0,
                    &mut errors,
                );
                validate_string_list(
                    "context.requiredCertifications",
                    &context.required_certifications,
                    20,
                    120,
                    &mut errors,
                );
                validate_optional_text(
                    "context.control",
                    context.control.as_deref(),
                    120,
                    &mut errors,
                );
            }
        }
        RfqJourney::Project => {
            reject_product_context(request, &mut errors);
            if let Some(context) =
                parse_rfq_context::<ProjectRfqContext>(context_value, "project", &mut errors)
            {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
                if !["Concept", "Engineering", "Prototype", "Production planning"]
                    .contains(&context.project_stage.as_str())
                {
                    errors.insert(
                        "context.projectStage".into(),
                        vec!["Project stage must be one of the supported values.".into()],
                    );
                }
                validate_optional_text(
                    "context.projectScale",
                    context.project_scale.as_deref(),
                    500,
                    &mut errors,
                );
                validate_optional_text(
                    "context.schedule",
                    context.schedule.as_deref(),
                    500,
                    &mut errors,
                );
                validate_optional_text(
                    "context.engineeringNeeds",
                    context.engineering_needs.as_deref(),
                    4_000,
                    &mut errors,
                );
            }
        }
        RfqJourney::Replacement => {
            reject_product_context(request, &mut errors);
            if let Some(context) = parse_rfq_context::<ReplacementRfqContext>(
                context_value,
                "replacement",
                &mut errors,
            ) {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
                validate_required_text(
                    "context.existingModel",
                    &context.existing_model,
                    300,
                    &mut errors,
                );
                validate_duty_point(&context.duty_point, &mut errors);
                validate_optional_text(
                    "context.installationConstraints",
                    context.installation_constraints.as_deref(),
                    4_000,
                    &mut errors,
                );
                validate_optional_text(
                    "context.replacementGoal",
                    context.replacement_goal.as_deref(),
                    2_000,
                    &mut errors,
                );
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn parse_rfq_context<T: for<'de> Deserialize<'de>>(
    value: Value,
    journey: &str,
    errors: &mut BTreeMap<String, Vec<String>>,
) -> Option<T> {
    match serde_json::from_value(value) {
        Ok(context) => Some(context),
        Err(_) => {
            errors.entry("context".into()).or_default().push(format!(
                "Context does not match the typed {journey} RFQ contract."
            ));
            None
        }
    }
}

fn reject_product_context(request: &CreateRfqRequest, errors: &mut BTreeMap<String, Vec<String>>) {
    if request.product_context.is_some() {
        errors.insert(
            "productContext".into(),
            vec!["Product context is accepted only for a Product RFQ.".into()],
        );
    }
}

fn validate_product_context(
    context: &crate::models::ProductContext,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    validate_required_text("productContext.stableId", &context.stable_id, 200, errors);
    match context.model.as_deref() {
        Some(model) => validate_required_text("productContext.model", model, 200, errors),
        None => {
            errors.insert(
                "productContext.model".into(),
                vec!["Product RFQ requires the published model identifier.".into()],
            );
        }
    }
    if context.published_revision <= 0 {
        errors.insert(
            "productContext.publishedRevision".into(),
            vec!["Published revision must be a positive integer.".into()],
        );
    }
}

fn validate_common_rfq_context(
    application: &str,
    quantity: Option<&RfqQuantity>,
    electrical: Option<&RfqElectricalContext>,
    environment: Option<&str>,
    priority: Option<&str>,
    additional_message: Option<&str>,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    validate_required_text("context.application", application, 500, errors);
    if let Some(quantity) = quantity {
        let valid = match quantity {
            RfqQuantity::Integer(value) => (1..=1_000_000).contains(value),
            RfqQuantity::Text(value) => {
                !value.is_empty()
                    && value.len() <= 7
                    && value.bytes().all(|byte| byte.is_ascii_digit())
                    && value
                        .parse::<u64>()
                        .is_ok_and(|value| (1..=1_000_000).contains(&value))
            }
        };
        if !valid {
            errors.insert(
                "context.quantity".into(),
                vec!["Quantity must be a whole number from 1 to 1,000,000.".into()],
            );
        }
    }
    if let Some(electrical) = electrical {
        if electrical.voltage.is_none() && electrical.frequency_hz.is_none() {
            errors.insert(
                "context.electrical".into(),
                vec!["Electrical context must contain voltage or frequencyHz.".into()],
            );
        }
        validate_optional_text(
            "context.electrical.voltage",
            electrical.voltage.as_deref(),
            40,
            errors,
        );
        validate_optional_finite_positive(
            "context.electrical.frequencyHz",
            electrical.frequency_hz,
            1_000.0,
            errors,
        );
    }
    validate_optional_text("context.environment", environment, 4_000, errors);
    if priority.is_some_and(|value| !["efficiency", "noise", "size", "headroom"].contains(&value)) {
        errors.insert(
            "context.priority".into(),
            vec!["Priority is not supported.".into()],
        );
    }
    validate_optional_text(
        "context.additionalMessage",
        additional_message,
        10_000,
        errors,
    );
}

fn validate_duty_point(duty_point: &RfqDutyPoint, errors: &mut BTreeMap<String, Vec<String>>) {
    validate_optional_finite_positive(
        "context.dutyPoint.airflow",
        Some(duty_point.airflow),
        1_000_000_000.0,
        errors,
    );
    validate_optional_finite_positive(
        "context.dutyPoint.pressure",
        Some(duty_point.pressure),
        100_000_000.0,
        errors,
    );
    if !["m3/h", "m³/h", "cfm"].contains(&duty_point.airflow_unit.to_lowercase().as_str()) {
        errors.insert(
            "context.dutyPoint.airflowUnit".into(),
            vec!["Unsupported airflow unit.".into()],
        );
    }
    if !["pa", "kpa", "inh2o"].contains(&duty_point.pressure_unit.to_lowercase().as_str()) {
        errors.insert(
            "context.dutyPoint.pressureUnit".into(),
            vec!["Unsupported pressure unit.".into()],
        );
    }
}

fn validate_optional_finite_positive(
    field: &str,
    value: Option<f64>,
    maximum: f64,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0 || value > maximum) {
        errors.insert(
            field.into(),
            vec![format!(
                "Must be a positive finite value no greater than {maximum}."
            )],
        );
    }
}

fn validate_string_list(
    field: &str,
    values: &[String],
    maximum_items: usize,
    maximum_length: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    let valid = values.len() <= maximum_items
        && values.iter().all(|value| {
            let value = value.trim();
            !value.is_empty()
                && value.chars().count() <= maximum_length
                && !value.chars().any(char::is_control)
        });
    if !valid {
        errors.insert(
            field.into(),
            vec!["The list contains too many or invalid values.".into()],
        );
    }
}

fn contains_attachment_field(value: &Value) -> bool {
    const ATTACHMENT_KEYS: &[&str] = &[
        "attachment",
        "attachments",
        "attachmentname",
        "attachmenturl",
        "file",
        "files",
        "filename",
        "filenames",
        "fileurl",
        "upload",
        "uploads",
        "uploadid",
    ];
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let normalized = key
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>();
            ATTACHMENT_KEYS.contains(&normalized.as_str()) || contains_attachment_field(value)
        }),
        Value::Array(values) => values.iter().any(contains_attachment_field),
        _ => false,
    }
}

fn validate_contact(request: &CreateContactRequest) -> Result<(), ApiError> {
    let mut errors = validate_business_contact(&request.contact);
    if request.topic.trim().is_empty() {
        errors.insert("topic".into(), vec!["Topic is required.".into()]);
    }
    if request.message.trim().len() < 10 || request.message.len() > 10_000 {
        errors.insert(
            "message".into(),
            vec!["Message must contain between 10 and 10,000 characters.".into()],
        );
    }
    if !request.consent {
        errors.insert("consent".into(), vec!["Consent is required.".into()]);
    }
    validate_public_source_and_locale(
        &request.source_path,
        &request.locale,
        "/en/company/contact",
        &mut errors,
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_business_contact(
    contact: &crate::models::BusinessContact,
) -> BTreeMap<String, Vec<String>> {
    let mut errors = BTreeMap::new();
    if !valid_text(&contact.name, 200) {
        errors.insert(
            "contact.name".into(),
            vec!["A valid name is required.".into()],
        );
    }
    let email = contact.email.trim();
    let email_parts = email.split('@').collect::<Vec<_>>();
    if email.len() > 320
        || email_parts.len() != 2
        || email_parts[0].is_empty()
        || !email_parts[1].contains('.')
        || email.chars().any(char::is_whitespace)
        || email.chars().any(char::is_control)
    {
        errors.insert(
            "contact.email".into(),
            vec!["A valid email address is required.".into()],
        );
    }
    if contact.phone.as_deref().is_some_and(|value| {
        !valid_text(value, 50)
            || !(5..=20).contains(&value.bytes().filter(u8::is_ascii_digit).count())
    }) {
        errors.insert(
            "contact.phone".into(),
            vec!["Phone must contain 5 to 20 digits and no more than 50 characters.".into()],
        );
    }
    validate_optional_text(
        "contact.company",
        contact.company.as_deref(),
        200,
        &mut errors,
    );
    validate_optional_text(
        "contact.countryOrRegion",
        contact.country_or_region.as_deref(),
        120,
        &mut errors,
    );
    errors
}

fn validate_public_source_and_locale(
    source_path: &str,
    locale: &str,
    expected_prefix: &str,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if source_path.len() > 2_048
        || !source_path.starts_with(expected_prefix)
        || source_path.contains(['?', '#'])
        || source_path.chars().any(char::is_control)
    {
        errors.insert(
            "sourcePath".into(),
            vec![
                "Source path must be a canonical public path without query or fragment data."
                    .into(),
            ],
        );
    }
    if locale != "en" {
        errors.insert(
            "locale".into(),
            vec!["Only the published English locale is currently accepted.".into()],
        );
    }
}

fn validate_required_text(
    field: &str,
    value: &str,
    maximum_length: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if !valid_text(value, maximum_length) {
        errors.insert(
            field.into(),
            vec![format!(
                "Must contain text no longer than {maximum_length} characters."
            )],
        );
    }
}

fn validate_optional_text(
    field: &str,
    value: Option<&str>,
    maximum_length: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if value.is_some_and(|value| !valid_text(value, maximum_length)) {
        errors.insert(
            field.into(),
            vec![format!(
                "When supplied, must contain text no longer than {maximum_length} characters."
            )],
        );
    }
}

fn valid_text(value: &str, maximum_length: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= maximum_length
        && !value.chars().any(char::is_control)
}

fn validate_analytics_event(event: &CreateAnalyticsEvent) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    let Some(allowed_properties) = analytics_property_dictionary(&event.event_name) else {
        errors.insert(
            "eventName".into(),
            vec!["Event name is not in the approved analytics dictionary.".into()],
        );
        return Err(ApiError::validation(errors));
    };
    if event.source_path.len() > 2_048
        || !event.source_path.starts_with("/en")
        || (event.source_path.len() > 3
            && !event
                .source_path
                .as_bytes()
                .get(3)
                .is_some_and(|byte| *byte == b'/'))
        || event.source_path.contains(['?', '#'])
        || event.source_path.chars().any(char::is_control)
    {
        errors.insert(
            "sourcePath".into(),
            vec!["Source path must be a canonical /en path without query or fragment data.".into()],
        );
    }
    if event.locale != "en" {
        errors.insert(
            "locale".into(),
            vec!["Only the published English locale is currently accepted.".into()],
        );
    }
    if event.properties.len() > allowed_properties.len() {
        errors.insert(
            "properties".into(),
            vec!["Analytics event contains more properties than its approved schema.".into()],
        );
    }
    for (key, value) in &event.properties {
        let field = format!("properties.{key}");
        if !allowed_properties.contains(&key.as_str()) {
            errors.insert(
                field,
                vec!["Property is not allowed for this analytics event.".into()],
            );
            continue;
        }
        if !analytics_scalar_is_valid(&event.event_name, key, value) {
            errors.insert(
                field,
                vec!["Property has an invalid type, value, length, or PII-like content.".into()],
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn analytics_property_dictionary(event_name: &str) -> Option<&'static [&'static str]> {
    match event_name {
        "pageView" => Some(&["contentKind", "contentId", "publishedRevision"]),
        "internalSearch" => Some(&["queryLength", "resultCount"]),
        "filterApplied" => Some(&["filterName", "filterValue", "resultCount"]),
        "selectorStarted" => Some(&["constraintCount", "preferredFamily", "priority"]),
        "selectorStepCompleted" => Some(&["step", "constraintCount"]),
        "selectorResult" => Some(&["outcome", "candidateCount"]),
        "compareChanged" => Some(&["action", "itemCount", "productId", "productRevision"]),
        "downloadStarted" => Some(&["downloadId", "productId", "productRevision"]),
        "faqExpanded" => Some(&["faqId", "category"]),
        "ctaClicked" => Some(&["ctaId", "destinationPath", "placement"]),
        "rfqRouteSelected" => Some(&["journey"]),
        "rfqStarted" => Some(&["journey", "productId", "productRevision"]),
        "rfqStepCompleted" => Some(&["journey", "step"]),
        "rfqValidationError" => Some(&["journey", "step", "fieldName", "errorCode"]),
        "rfqSubmitted" => Some(&["journey"]),
        "rfqSubmitFailed" => Some(&["journey", "errorCode"]),
        _ => None,
    }
}

fn analytics_scalar_is_valid(event_name: &str, key: &str, value: &Value) -> bool {
    match value {
        Value::String(value) => {
            if !valid_analytics_string(value, 160) {
                return false;
            }
            match key {
                "contentId" | "productId" | "downloadId" => Uuid::parse_str(value).is_ok(),
                "destinationPath" => {
                    value.len() <= 512
                        && (value == "/en" || value.starts_with("/en/"))
                        && !value.contains(['?', '#'])
                }
                "journey" => {
                    ["product", "selection", "project", "replacement"].contains(&value.as_str())
                }
                "priority" => ["efficiency", "noise", "size", "headroom"].contains(&value.as_str()),
                "preferredFamily" => [
                    "open",
                    "centrifugal",
                    "axial",
                    "crossFlow",
                    "inlineDuct",
                    "motors",
                ]
                .contains(&value.as_str()),
                "outcome" => [
                    "matched",
                    "noValidatedCandidates",
                    "engineeringReviewRequired",
                ]
                .contains(&value.as_str()),
                "action" if event_name == "compareChanged" => {
                    ["add", "remove", "clear"].contains(&value.as_str())
                }
                "contentKind" | "filterName" | "faqId" | "ctaId" | "placement" | "fieldName"
                | "errorCode" => valid_analytics_identifier(value),
                "filterValue" | "category" => true,
                _ => false,
            }
        }
        Value::Number(value) => {
            let Some(value) = value.as_u64() else {
                return false;
            };
            match key {
                "queryLength" => (1..=500).contains(&value),
                "resultCount" | "candidateCount" | "constraintCount" => value <= 1_000_000,
                "itemCount" => value <= 4,
                "step" => (1..=20).contains(&value),
                "publishedRevision" | "productRevision" => (1..=i64::MAX as u64).contains(&value),
                _ => false,
            }
        }
        // The approved dictionary currently has no nullable, boolean, array,
        // or object-valued properties. Reject them instead of recursively
        // accepting a future source of form content or identifiers.
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => false,
    }
}

fn valid_analytics_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_analytics_string(value: &str, maximum_length: usize) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.chars().count() <= maximum_length
        && !value.chars().any(char::is_control)
        && !looks_like_email(value)
        && !looks_like_phone(value)
        && value.parse::<std::net::IpAddr>().is_err()
        && !looks_like_secret(value)
}

fn looks_like_email(value: &str) -> bool {
    value.split_whitespace().any(|token| {
        let token = token.trim_matches(|character: char| {
            matches!(character, '<' | '>' | '(' | ')' | '[' | ']' | ',' | ';')
        });
        token.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
    })
}

fn looks_like_phone(value: &str) -> bool {
    let digit_count = value.bytes().filter(u8::is_ascii_digit).count();
    (8..=15).contains(&digit_count)
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b' ' | b'+' | b'-' | b'(' | b')' | b'.')
        })
}

fn looks_like_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("basic ")
        || lower.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("token=")
}

fn reference(prefix: &str, id: Uuid, at: chrono::DateTime<Utc>) -> String {
    let compact = id.simple().to_string();
    format!("{prefix}-{}-{}", at.format("%Y%m%d"), &compact[..8]).to_uppercase()
}

fn default_locale() -> String {
    "en".into()
}
