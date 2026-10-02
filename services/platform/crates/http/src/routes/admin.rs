use std::{cmp::Reverse, convert::Infallible, time::Duration};

use axum::{
    extract::{rejection::JsonRejection, Extension, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{sse::Event, sse::KeepAlive, IntoResponse, Response, Sse},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{Duration as ChronoDuration, Utc};
use futures_util::stream;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::routes::{actor, etag, parse_if_match};
use airtek_domain::models::{
    AnalyticsOverview, AuditEvent, BackgroundOperation, ContentDraftV2, CreateTemporaryOverride,
    CursorPage, ObjectStorageSettingsInput, OperationKind, OperationStatus, Product,
    PublicationStatus, SyncRun, TemporaryOverride, UpdateObjectStorageSettings,
    UpdatePlatformSettings,
};
use airtek_runtime::auth::AdminPrincipal;
use airtek_runtime::error::ApiError;
use airtek_runtime::idempotency::{begin as begin_idempotency, IdempotencyOutcome};
use airtek_runtime::pagination::CursorQuery;
use airtek_runtime::services::cms_content;
use airtek_runtime::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/content-drafts",
            get(list_private_drafts).post(create_private_draft),
        )
        .route("/content-drafts/templates", get(list_content_templates))
        .route(
            "/content-drafts/{draftId}",
            get(get_private_draft).patch(save_private_draft),
        )
        .route(
            "/content-drafts/{draftId}/shares",
            put(set_private_draft_shares),
        )
        .route("/content-drafts/{draftId}/claim", post(claim_private_draft))
        .route(
            "/content-drafts/{draftId}/submit",
            post(submit_private_draft),
        )
        .route(
            "/content-drafts/{draftId}/withdraw",
            post(withdraw_private_draft),
        )
        .route("/content-reviews", get(list_content_reviews))
        .route("/site-singletons/{kind}", get(get_site_singleton))
        .route(
            "/content-reviews/{draftId}/approve",
            post(approve_content_review),
        )
        .route(
            "/content-reviews/{draftId}/reject",
            post(reject_content_review),
        )
        .route("/published-content", get(list_published_content))
        .route("/published-content/{contentId}", get(get_published_content))
        .route(
            "/published-content/{contentId}/drafts",
            post(copy_published_content_to_draft),
        )
        .route(
            "/media/assets",
            get(list_media_assets).post(upload_media_asset).layer(
                axum::extract::DefaultBodyLimit::max(
                    airtek_runtime::services::media::MAX_MEDIA_UPLOAD_BYTES + 64 * 1024,
                ),
            ),
        )
        .route("/media/assets/{id}", get(get_media_asset))
        .route("/media/assets/{id}/references", get(list_media_references))
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
        .route("/feishu/sync-runs/{id}", get(get_sync_run))
        .route(
            "/feishu/settings",
            get(get_feishu_settings_route).put(update_feishu_settings_route),
        )
        .route(
            "/feishu/connection-test",
            post(test_feishu_connection_route),
        )
        .route(
            "/feishu/connection-status",
            get(get_feishu_connection_status),
        )
        .route("/feishu/mappings", get(list_feishu_mappings))
        .route("/feishu/staging", get(list_feishu_staging))
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
        .route("/dashboard/summary", get(admin_dashboard_summary))
        .route("/settings", get(get_settings).patch(update_settings))
        .route(
            "/settings/object-storage",
            get(get_object_storage_settings).put(update_object_storage_settings),
        )
        .route(
            "/settings/object-storage/test",
            post(test_object_storage_settings),
        )
        .route(
            "/settings/mail",
            get(get_mail_settings).put(update_mail_settings),
        )
        .route("/settings/mail/test", post(test_mail_settings))
        .route(
            "/settings/sms",
            get(get_sms_settings).put(update_sms_settings),
        )
        .route("/settings/sms/test", post(test_sms_settings))
        .route(
            "/settings/captcha",
            get(get_captcha_settings).put(update_captcha_settings),
        )
        .route("/settings/captcha/test", post(test_captcha_settings))
        .route("/operations/{id}", get(get_operation))
        .route("/operations/{id}/events", get(operation_events))
        .route("/audit", get(list_audit))
        .route("/audit/export.csv", get(export_audit_csv))
        .merge(super::admin_data::router())
}

#[path = "admin/settings.rs"]
mod settings;
use settings::*;
#[path = "admin/cms_workflow.rs"]
mod cms_workflow;
use cms_workflow::*;
#[path = "admin/products.rs"]
mod products;
use products::*;
#[path = "admin/temporary_overrides.rs"]
mod temporary_overrides;
use temporary_overrides::*;
#[path = "admin/sync.rs"]
mod sync;
use sync::*;
#[path = "admin/submissions.rs"]
mod submissions;
use submissions::*;
#[path = "admin/submissions_support.rs"]
mod submissions_support;
use submissions_support::*;
#[path = "admin/analytics_overview.rs"]
mod analytics_overview;
use analytics_overview::*;
#[path = "admin/media.rs"]
mod media;
use media::*;
#[path = "admin/dashboard.rs"]
mod dashboard;
use dashboard::*;
#[path = "admin/operations.rs"]
mod operations;
use operations::*;
#[path = "admin/audit_and_validation.rs"]
mod audit_and_validation;
use audit_and_validation::*;
#[path = "admin/response_and_audit.rs"]
mod response_and_audit;
use response_and_audit::*;

fn parse_query_text(value: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > 200 {
        return Err(ApiError::bad_request("q must be at most 200 characters."));
    }
    Ok(Some(trimmed.to_owned()))
}

fn parse_content_if_match(headers: &HeaderMap) -> Result<i64, ApiError> {
    headers
        .get(header::IF_MATCH)
        .and_then(|value| value.to_str().ok())
        .and_then(cms_content::parse_draft_etag)
        .ok_or_else(|| {
            ApiError::precondition_required(
                "If-Match is required and must use the current draft-N ETag.",
            )
        })
}
