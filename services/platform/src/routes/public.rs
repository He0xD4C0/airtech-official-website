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

use crate::services::public_content::load_published_product_rows;
use crate::{
    error::ApiError,
    idempotency::{begin as begin_idempotency, IdempotencyOutcome},
    models::{
        AcceptedResponse, AnalyticsConsentReceipt, AnalyticsEventReceipt, CmsContentKind,
        ContactRequest, CreateAnalyticsConsent, CreateAnalyticsEvent, CreateContactRequest,
        CreateRfqRequest, CursorPage, Product, ProductFamily, ProductQuery, RfqJourney,
        RfqSubmission, SelectorRequest, SelectorResponse,
    },
    rate_limit::{
        enforce_public_rate_limit, ANALYTICS_CONSENT_POLICY, ANALYTICS_POLICY, CONTACT_POLICY,
        RFQ_POLICY,
    },
    routes::etag,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/content/{kind}/{slug}", get(get_content))
        .route("/products", get(list_products))
        .route("/products/{slug}/assets", get(get_product_assets))
        .route("/products/{slug}", get(get_product))
        .route("/discovery", get(discovery))
        .route("/media/{assetId}", get(get_public_media))
        .route("/media/{assetId}/download", get(download_public_media))
        .route("/selector", post(select_products))
        .route("/contact", post(create_contact))
        .route("/rfqs", post(create_rfq))
        .route("/analytics/consents", post(create_analytics_consent))
        .route("/analytics/events", post(create_analytics_event))
        .merge(super::public_data::router())
}

pub const ANALYTICS_POLICY_VERSION: &str = "analytics-v1";
const ANALYTICS_CONSENT_LIFETIME_DAYS: i64 = 180;

#[path = "public/discovery.rs"]
mod discovery;
use discovery::*;
#[path = "public/content_and_products.rs"]
mod content_and_products;
use content_and_products::*;
#[path = "public/submissions.rs"]
mod submissions;
use submissions::*;
#[path = "public/published_data.rs"]
mod published_data;
use published_data::*;
#[path = "public/media.rs"]
mod media;
use media::*;
#[path = "public/selector_and_rfq_validation.rs"]
mod selector_and_rfq_validation;
use selector_and_rfq_validation::*;
#[path = "public/rfq_context_validation.rs"]
mod rfq_context_validation;
use rfq_context_validation::*;
#[path = "public/contact_validation.rs"]
mod contact_validation;
use contact_validation::*;
#[path = "public/analytics_validation.rs"]
mod analytics_validation;
use analytics_validation::*;
#[cfg(test)]
#[path = "public/tests.rs"]
mod tests;
