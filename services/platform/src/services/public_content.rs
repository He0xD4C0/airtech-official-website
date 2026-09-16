use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        CmsContentKind, DataClass, GuestVisit, NewsEntry, Product, ProductFamily,
        PublicContentProjection,
    },
    state::AppState,
};

#[path = "public_content/discovery.rs"]
mod discovery;
#[path = "public_content/products.rs"]
mod products;
#[path = "public_content/projection.rs"]
mod projection;
#[path = "public_content/routes.rs"]
mod routes;
#[path = "public_content/visits.rs"]
mod visits;

pub(crate) use discovery::*;
pub(crate) use products::*;
pub(crate) use projection::*;
pub(crate) use routes::*;
pub(crate) use visits::*;
