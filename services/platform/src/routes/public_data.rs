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
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::Config,
    error::ApiError,
    models::{
        CmsContentKind, ContentTypeFields, CreateGuestVisit, CursorPage, DataClass, NewsEntry,
        ProductFamilyPresentation, PublicContentProjection, RouteResolution, SiteBootstrap,
    },
    pagination::{decode_scoped_cursor, encode_scoped_cursor},
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

#[path = "public_data/bootstrap.rs"]
mod bootstrap;
pub(crate) use bootstrap::published_site_shell_has_placeholder;
use bootstrap::*;
#[path = "public_data/motor_technologies.rs"]
mod motor_technologies;
use motor_technologies::*;
#[path = "public_data/route_resolution.rs"]
mod route_resolution;
use route_resolution::*;
#[path = "public_data/news_and_visits.rs"]
mod news_and_visits;
use news_and_visits::*;
#[path = "public_data/published_loaders.rs"]
mod published_loaders;
pub(crate) use crate::services::public_content::load_v2_content_by_kind_slug;
use crate::services::public_content::{
    load_v2_news, load_v2_news_cursor_position, load_v2_route, load_v2_singleton,
};
use published_loaders::*;
#[path = "public_data/presentations.rs"]
mod presentations;
use presentations::*;
#[path = "public_data/privacy_validation.rs"]
mod privacy_validation;
pub(crate) use privacy_validation::valid_guest_landing_path;
use privacy_validation::*;
#[path = "public_data/content_helpers.rs"]
mod content_helpers;
use content_helpers::*;
#[cfg(test)]
#[path = "public_data/tests.rs"]
mod tests;
