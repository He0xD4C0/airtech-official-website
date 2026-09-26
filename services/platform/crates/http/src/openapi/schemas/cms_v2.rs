//! Strict CMS V2 OpenAPI component schemas.

mod blocks;
mod core;
mod fields;
mod service;

use serde_json::{Map, Value};

pub(super) fn add(schemas: &mut Map<String, Value>) {
    core::add(schemas);
    blocks::add(schemas);
    fields::add(schemas);
    service::add(schemas);
}

#[cfg(test)]
mod tests;
