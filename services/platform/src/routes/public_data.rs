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
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    config::Config,
    error::ApiError,
    models::{
        ContentEntry, ContentKind, CreateGuestVisit, CursorPage, DataClass, GeneralInformation,
        GuestVisit, NewsEntry, ProductFamily, ProductFamilyPresentation, PublicationStatus,
        RouteResolution, SiteBootstrap,
    },
    routes::etag,
    state::AppState,
};

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
include!("public_data/presentations.rs");
include!("public_data/privacy_validation.rs");
include!("public_data/content_helpers.rs");
include!("public_data/tests.rs");
