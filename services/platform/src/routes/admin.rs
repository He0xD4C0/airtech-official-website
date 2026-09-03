use std::{cmp::Reverse, collections::BTreeMap, convert::Infallible, time::Duration};

use axum::{
    extract::{rejection::JsonRejection, Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{sse::Event, sse::KeepAlive, IntoResponse, Response, Sse},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{DateTime, Duration as ChronoDuration, NaiveTime, Utc};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::AdminPrincipal,
    error::ApiError,
    idempotency::{begin as begin_idempotency, IdempotencyOutcome},
    models::{
        AnalyticsBusinessOutcomes, AnalyticsConsentedMetrics, AnalyticsOverview,
        AnalyticsOverviewRange, AuditEvent, BackgroundOperation, ContentDraftInput, ContentEntry,
        ContentKind, ContentPreviewLink, CreateContentPreviewRequest, CreateOperationRequest,
        CreateTemporaryOverride, CursorPage, OperationKind, OperationStatus, Product,
        PublicationStatus, StartSyncRequest, SyncRun, SyncRunStatus, TemporaryOverride,
        UpdatePlatformSettings,
    },
    pagination::{paginate_by_id, CursorQuery},
    routes::{actor, etag, parse_if_match},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/content", get(list_content).post(create_content))
        .route("/content/{id}", patch(update_content))
        .route("/content/{id}/preview", post(create_content_preview))
        .route("/content/{id}/publish", post(publish_content))
        .route("/content/{id}/rollback", post(rollback_content))
        .route("/products", get(list_products))
        .route("/products/{id}/publish", post(publish_product))
        .route(
            "/products/{id}/temporary-overrides",
            get(list_temporary_overrides).post(create_temporary_override),
        )
        .route(
            "/feishu/sync-runs",
            get(list_sync_runs).post(start_sync_run),
        )
        .route("/feishu/conflicts", get(list_conflicts))
        .route("/rfqs", get(list_rfqs))
        .route("/contacts", get(list_contacts))
        .route("/analytics/overview", get(analytics_overview))
        .route("/analytics/summary", get(analytics_summary))
        .route("/settings", get(get_settings).patch(update_settings))
        .route("/operations", get(list_operations).post(create_operation))
        .route("/operations/{id}", get(get_operation))
        .route("/operations/{id}/events", get(operation_events))
        .route("/audit", get(list_audit))
        .merge(super::admin_data::router())
}

include!("admin/settings.rs");
include!("admin/content_preview.rs");
include!("admin/content.rs");
include!("admin/products.rs");
include!("admin/temporary_overrides.rs");
include!("admin/sync.rs");
include!("admin/submissions.rs");
include!("admin/analytics_overview.rs");
include!("admin/analytics_summary.rs");
include!("admin/operations.rs");
include!("admin/audit_and_validation.rs");
include!("admin/response_and_audit.rs");
