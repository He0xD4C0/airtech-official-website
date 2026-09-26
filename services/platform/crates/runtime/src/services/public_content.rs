use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use airtek_domain::models::{
    CmsContentKind, DataClass, GuestVisit, NewsEntry, Product, ProductFamily, ProductSourceAsset,
    ProductSourceAssetDocument, PublicContentProjection,
};

#[path = "public_content/discovery.rs"]
mod discovery;
#[path = "public_content/search.rs"]
mod search;
pub use search::search_public_site;
#[path = "public_content/products.rs"]
mod products;
#[path = "public_content/projection.rs"]
mod projection;
#[path = "public_content/routes.rs"]
mod routes;
#[path = "public_content/visits.rs"]
mod visits;

pub use discovery::*;
pub use products::*;
pub use projection::*;
pub use routes::*;
pub use visits::*;
