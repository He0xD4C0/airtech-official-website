//! OpenAPI component-schema assembly.

mod admin;
mod analytics;
mod cms_v2;
mod common;
mod content;
mod management;
mod product;
mod site;
mod submissions;

use serde_json::{Map, Value};

pub(super) fn build() -> Value {
    let mut schemas = Map::new();
    common::add(&mut schemas);
    content::add(&mut schemas);
    cms_v2::add(&mut schemas);
    product::add(&mut schemas);
    submissions::add(&mut schemas);
    site::add(&mut schemas);
    analytics::add(&mut schemas);
    management::add(&mut schemas);
    admin::add(&mut schemas);
    Value::Object(schemas)
}
