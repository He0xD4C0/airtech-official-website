use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
};

use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    config::Config,
    error::ApiError,
    models::{
        CmsContentKind, ContentTypeFields, CreateGuestVisit, CursorPage, DataClass, GuestVisit,
        NewsEntry, ProductFamilyPresentation, PublicContentProjection, RouteResolution,
        SiteBootstrap,
    },
    routes::etag,
    state::AppState,
};

#[cfg(test)]
use crate::models::ProductFamily;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/site-bootstrap", get(site_bootstrap))
        .route("/routes/resolve", get(resolve_route))
        .route("/news", get(list_news))
        .route("/news/{slug}", get(get_news))
        .route("/guest-visits", post(create_guest_visit))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocaleQuery {
    #[serde(default = "default_locale")]
    locale: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveRouteQuery {
    path: String,
    #[serde(default = "default_locale")]
    locale: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewsQuery {
    #[serde(default = "default_locale")]
    locale: String,
    category: Option<String>,
    cursor: Option<String>,
    limit: Option<usize>,
}

include!("public_data/bootstrap.rs");
include!("public_data/motor_technologies.rs");
include!("public_data/route_resolution.rs");
include!("public_data/news_and_visits.rs");
include!("public_data/published_loaders.rs");
include!("public_data/v2_projection.rs");
include!("public_data/presentations.rs");
include!("public_data/privacy_validation.rs");
include!("public_data/content_helpers.rs");
include!("public_data/tests.rs");
