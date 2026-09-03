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
use sqlx::Row;
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
        .merge(super::public_data::router())
}

pub const ANALYTICS_POLICY_VERSION: &str = "analytics-v1";
const ANALYTICS_CONSENT_LIFETIME_DAYS: i64 = 180;

include!("public/preview.rs");
include!("public/discovery.rs");
include!("public/content_and_products.rs");
include!("public/submissions.rs");
include!("public/published_data.rs");
include!("public/selector_and_rfq_validation.rs");
include!("public/rfq_context_validation.rs");
include!("public/contact_validation.rs");
include!("public/analytics_validation.rs");
include!("public/tests.rs");
