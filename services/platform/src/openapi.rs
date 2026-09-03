mod paths;
mod schemas;
mod support;

use serde_json::{json, Map, Value};

/// Builds the contract from the same feature set as the compiled API.
///
/// The checked-in snapshot is exported with the `production` feature, which
/// makes development-only routes impossible to publish accidentally.
pub fn document() -> Value {
    let mut paths = Map::new();
    paths::add_all(&mut paths);

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "AIRTEKPOWER Platform API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Public and admin API contract. Exact product facts require a validated Feishu source snapshot."
        },
        "servers": [{"url": "http://localhost:8080", "description": "Fixed local application port"}],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "adminSession": {"type": "apiKey", "in": "cookie", "name": "airtek_admin_session", "description": "Host-only HttpOnly cookie scoped to /api."},
                "csrfToken": {"type": "apiKey", "in": "header", "name": "X-CSRF-Token", "description": "Required with the admin session on protected mutations."},
                "previewToken": {"type": "http", "scheme": "bearer", "bearerFormat": "AIRTEK preview v1", "description": "Short-lived signed capability for exactly one content revision, bound to its issuing Admin user and session. Every read revalidates the active user, unrevoked and unexpired session, confirmed TOTP, and current content.read permission in PostgreSQL. Never send it in a query parameter to the API."}
            },
            "schemas": schemas::build()
        },
        "x-airtek-conventions": {
            "jsonNaming": "camelCase",
            "timestamps": "UTC RFC3339",
            "identifiers": "UUID",
            "pagination": "cursor",
            "concurrency": "ETag and If-Match",
            "replaySafety": "Idempotency-Key",
            "longRunning": "202 operationId"
        },
        "x-airtek-build": if cfg!(feature = "devtools") { "development-with-devtools" } else if cfg!(feature = "production") { "production" } else { "development" }
    })
}

#[cfg(test)]
mod tests;
