use std::{cmp::Reverse, collections::BTreeMap, convert::Infallible, time::Duration};

use axum::{
    extract::{rejection::JsonRejection, Extension, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{sse::Event, sse::KeepAlive, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration as ChronoDuration, NaiveTime, Utc};
use futures_util::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    auth::AdminPrincipal,
    error::ApiError,
    idempotency::{begin as begin_idempotency, IdempotencyOutcome},
    models::{
        AnalyticsBusinessOutcomes, AnalyticsConsentedMetrics, AnalyticsOverview,
        AnalyticsOverviewRange, ArchiveContentRequest, AuditEvent, BackgroundOperation,
        ContentDraftV2, ContentRecordV2, ContentRevisionV2, ContentSnapshotIntent,
        CreateContentSnapshotRequest, CreateOperationRequest, CreateTemporaryOverride, CursorPage,
        OperationKind, OperationStatus, Product, PublicationStatus, RestoreContentRevisionRequest,
        StartSyncRequest, SyncRun, SyncRunStatus, TemporaryOverride, UnpublishContentRequest,
        UpdatePlatformSettings,
    },
    pagination::{paginate_by_id, CursorQuery},
    routes::{actor, etag, parse_if_match},
    services::cms_content::{self, MutationMetadata},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/content", get(list_content).post(create_content))
        .route("/content/templates", get(list_content_templates))
        .route(
            "/media/assets",
            get(list_media_assets).post(upload_media_asset).layer(
                axum::extract::DefaultBodyLimit::max(
                    crate::services::media::MAX_MEDIA_UPLOAD_BYTES + 64 * 1024,
                ),
            ),
        )
        .route("/media/assets/{id}", get(get_media_asset))
        .route("/media/assets/{id}/references", get(list_media_references))
        .route(
            "/content/{id}/draft",
            get(get_content_draft).patch(update_content_draft),
        )
        .route(
            "/content/{id}/publication-readiness",
            get(get_content_publication_readiness),
        )
        .route("/content/{id}/publish", post(publish_content))
        .route("/content/{id}/snapshots", post(create_content_snapshot))
        .route("/content/{id}/unpublish", post(unpublish_content))
        .route("/content/{id}/archive", post(archive_content))
        .route("/content/{id}/revisions", get(list_content_revisions))
        .route("/content/{id}/diff", get(get_content_diff))
        .route(
            "/content/{id}/revisions/{revision}/restore",
            post(restore_content_revision),
        )
        .route("/products", get(list_products))
        .route(
            "/products/{id}/publication-readiness",
            get(get_product_publication_report),
        )
        .route(
            "/products/{id}/validation-report",
            get(get_product_publication_report),
        )
        .route("/products/{id}/publish", post(publish_product))
        .route(
            "/products/{id}/temporary-overrides",
            get(list_temporary_overrides).post(create_temporary_override),
        )
        .route(
            "/feishu/sync-runs",
            get(list_sync_runs).post(start_sync_run),
        )
        .route(
            "/feishu/connection-status",
            get(get_feishu_connection_status),
        )
        .route("/feishu/mappings", get(list_feishu_mappings))
        .route("/feishu/staging", get(list_feishu_staging))
        .route("/feishu/conflicts", get(list_conflicts))
        .route("/feishu/conflicts/{id}/resolve", post(resolve_conflict))
        .route("/rfqs", get(list_rfqs))
        .route("/rfqs/{id}", get(get_rfq))
        .route("/rfqs/{id}/pii", get(get_rfq_pii))
        .route("/rfqs/{id}/assignment", post(assign_rfq))
        .route("/rfqs/{id}/status", post(update_rfq_status))
        .route("/rfqs/{id}/notes", post(add_rfq_note))
        .route("/contacts", get(list_contacts))
        .route("/contacts/{id}", get(get_contact))
        .route("/contacts/{id}/pii", get(get_contact_pii))
        .route("/contacts/{id}/assignment", post(assign_contact))
        .route("/contacts/{id}/status", post(update_contact_status))
        .route("/contacts/{id}/notes", post(add_contact_note))
        .route("/analytics/overview", get(analytics_overview))
        .route("/analytics/summary", get(analytics_summary))
        .route("/dashboard/summary", get(admin_dashboard_summary))
        .route("/settings", get(get_settings).patch(update_settings))
        .route("/operations", get(list_operations).post(create_operation))
        .route("/operations/{id}", get(get_operation))
        .route("/operations/{id}/events", get(operation_events))
        .route("/audit", get(list_audit))
        .route("/audit/export.csv", get(export_audit_csv))
        .merge(super::admin_data::router())
}

include!("admin/settings.rs");
include!("admin/cms_content.rs");
include!("admin/cms_content_lifecycle.rs");
include!("admin/products.rs");
include!("admin/temporary_overrides.rs");
include!("admin/sync.rs");
include!("admin/sync_conflicts.rs");
include!("admin/submissions.rs");
include!("admin/submissions_support.rs");
include!("admin/analytics_overview.rs");
include!("admin/media.rs");
include!("admin/analytics_summary.rs");
include!("admin/dashboard.rs");
include!("admin/operations.rs");
include!("admin/audit_and_validation.rs");
include!("admin/response_and_audit.rs");
