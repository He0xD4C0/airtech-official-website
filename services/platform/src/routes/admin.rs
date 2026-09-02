use std::{cmp::Reverse, collections::BTreeMap, convert::Infallible, time::Duration};

use axum::{
    extract::{rejection::JsonRejection, Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{sse::Event, sse::KeepAlive, IntoResponse, Response, Sse},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::{Duration as ChronoDuration, Utc};
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
        AuditEvent, BackgroundOperation, ContentDraftInput, ContentEntry, ContentKind,
        ContentPreviewLink, CreateContentPreviewRequest, CreateOperationRequest,
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
        .route("/analytics/summary", get(analytics_summary))
        .route("/settings", get(get_settings).patch(update_settings))
        .route("/operations", get(list_operations).post(create_operation))
        .route("/operations/{id}", get(get_operation))
        .route("/operations/{id}/events", get(operation_events))
        .route("/audit", get(list_audit))
        .merge(super::admin_data::router())
}

async fn get_settings(State(state): State<AppState>) -> Result<Response, ApiError> {
    let settings = state.platform_settings().await?;
    let revision = settings.revision;
    let mut response = entity_response(StatusCode::OK, &settings, revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

async fn update_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<UpdatePlatformSettings>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(update) = payload.map_err(|_| {
        ApiError::bad_request(
            "Settings must be a valid JSON object containing only the documented mutable fields and reason.",
        )
    })?;
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    let settings = state
        .update_platform_settings(expected_revision, &update, &actor, request_id)
        .await?;
    let revision = settings.revision;
    let mut response = entity_response(StatusCode::OK, &settings, revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

async fn create_content_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<CreateContentPreviewRequest>,
) -> Result<Response, ApiError> {
    let expires_in_seconds = request
        .expires_in_seconds
        .unwrap_or(crate::preview_token::MAX_PREVIEW_TTL_SECONDS);
    if request.revision < 1 {
        return Err(ApiError::bad_request("revision must be at least 1."));
    }
    if !(1..=crate::preview_token::MAX_PREVIEW_TTL_SECONDS).contains(&expires_in_seconds) {
        return Err(ApiError::bad_request(format!(
            "expiresInSeconds must contain 1 to {}.",
            crate::preview_token::MAX_PREVIEW_TTL_SECONDS
        )));
    }

    let current = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    if headers.contains_key(header::IF_MATCH) {
        let expected_revision = parse_if_match(&headers)?;
        if expected_revision != current.current_revision {
            return Err(ApiError::conflict(
                "The content entry changed; reload before creating a preview.",
            ));
        }
    }

    let existing_snapshot = state.load_content_revision(id, request.revision).await?;
    if existing_snapshot.is_none() && request.revision == current.current_revision {
        state
            .snapshot_working_content(&current, &actor(&headers))
            .await?;
    } else if existing_snapshot.is_none() {
        return Err(ApiError::not_found(
            "The requested content revision was not found.",
        ));
    }
    let key = state.config.preview_signing_key.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Content preview signing is not configured.")
    })?;
    let issued = crate::preview_token::issue(
        key,
        id,
        request.revision,
        principal.user_id,
        principal.session_id,
        expires_in_seconds,
    )?;
    let preview = ContentPreviewLink {
        url: format!(
            "{}/en/preview?token={}",
            state.config.public_origin, issued.token
        ),
        content_id: id,
        revision: request.revision,
        issued_at: issued.issued_at,
        expires_at: issued.expires_at,
    };

    let actor = actor(&headers);
    audit(
        &state,
        &headers,
        &actor,
        "content.preview.issue",
        "content",
        Some(id),
        None,
        Some(json!({
            "revision": preview.revision,
            "issuedAt": preview.issued_at,
            "expiresAt": preview.expires_at,
        })),
        Some("Create a short-lived content preview link".into()),
    )
    .await?;

    let mut response = (StatusCode::CREATED, Json(preview.clone())).into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&preview.url)
            .map_err(|_| ApiError::internal("Preview URL is not a valid Location header."))?,
    );
    add_private_no_store_headers(&mut response);
    Ok(response)
}

async fn list_content(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<ContentEntry>>, ApiError> {
    let mut values = state.list_working_content().await?;
    // News has metadata and projection invariants that only the dedicated News
    // service can update transactionally. Keep it out of the generic editor.
    values.retain(|entry| entry.kind != ContentKind::News);
    values.sort_by_key(|entry| Reverse(entry.updated_at));
    Ok(Json(paginate_by_id(
        "admin.content",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn create_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<ContentDraftInput>,
) -> Result<Response, ApiError> {
    reject_generic_news_input(input.kind)?;
    validate_content_input(&input)?;
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.create",
        &headers,
        &json!({"actor": &actor, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if state
        .content_identity_exists(input.kind, &input.slug, &input.locale, None)
        .await?
    {
        return Err(ApiError::conflict(
            "A content entry already uses this kind, slug and locale.",
        ));
    }
    let now = Utc::now();
    let entry = ContentEntry {
        id: Uuid::new_v4(),
        kind: input.kind,
        slug: input.slug,
        locale: input.locale,
        title: input.title,
        summary: input.summary,
        body: input.body,
        seo: input.seo,
        status: PublicationStatus::Draft,
        is_placeholder: input.is_placeholder,
        current_revision: 1,
        published_revision: None,
        scheduled_for: None,
        updated_at: now,
    };
    state.persist_content(&entry, &actor).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .content
            .insert(entry.id, entry.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "content.create",
        "content",
        Some(entry.id),
        None,
        Some(json!(entry)),
        Some("Create working draft".into()),
    )
    .await?;
    idempotency
        .complete(&state, &entry, StatusCode::CREATED)
        .await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &entry,
        entry.current_revision,
    ))
}

async fn update_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<ContentDraftInput>,
) -> Result<Response, ApiError> {
    reject_generic_news_input(input.kind)?;
    validate_content_input(&input)?;
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let before = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    reject_generic_news_entry(&before)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.update",
        &headers,
        &json!({
            "actor": &actor,
            "id": id,
            "ifMatch": expected_revision,
            "input": &input,
        }),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if before.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The content entry changed; reload before saving.",
        ));
    }
    if state
        .content_identity_exists(input.kind, &input.slug, &input.locale, Some(id))
        .await?
    {
        return Err(ApiError::conflict(
            "A content entry already uses this kind, slug and locale.",
        ));
    }
    let mut updated = before.clone();
    updated.kind = input.kind;
    updated.slug = input.slug;
    updated.locale = input.locale;
    updated.title = input.title;
    updated.summary = input.summary;
    updated.body = input.body;
    updated.seo = input.seo;
    updated.is_placeholder = input.is_placeholder;
    updated.status = PublicationStatus::Draft;
    updated.current_revision += 1;
    updated.updated_at = Utc::now();

    state
        .persist_content_update(&updated, &actor, expected_revision)
        .await?;
    if state.pool.is_none() {
        state.data.write().await.content.insert(id, updated.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "content.update",
        "content",
        Some(id),
        Some(json!(before)),
        Some(json!(updated)),
        Some("Update working draft".into()),
    )
    .await?;
    idempotency
        .complete(&state, &updated, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &updated,
        updated.current_revision,
    ))
}

async fn publish_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let before = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    reject_generic_news_entry(&before)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.publish",
        &headers,
        &json!({"actor": &actor, "id": id, "ifMatch": expected_revision}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if before.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The content entry changed; reload before publishing.",
        ));
    }
    if before.status == PublicationStatus::Published
        && before.published_revision == Some(before.current_revision)
    {
        idempotency
            .complete(&state, &before, StatusCode::OK)
            .await?;
        return Ok(entity_response(
            StatusCode::OK,
            &before,
            before.current_revision,
        ));
    }
    let mut published = before.clone();
    published.status = PublicationStatus::Published;
    published.published_revision = Some(published.current_revision);
    published.updated_at = Utc::now();
    if published.is_placeholder {
        published.seo.indexable = false;
    }
    state
        .publish_content_projection(&published, &actor, "publish", expected_revision)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .content
            .insert(id, published.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "content.publish",
        "content",
        Some(id),
        Some(json!(before)),
        Some(json!(published)),
        Some("Publish current content revision".into()),
    )
    .await?;
    idempotency
        .complete(&state, &published, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &published,
        published.current_revision,
    ))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RollbackContentRequest {
    revision: i64,
    reason: String,
}

async fn rollback_content(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RollbackContentRequest>,
) -> Result<Response, ApiError> {
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let before = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    reject_generic_news_entry(&before)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.content.rollback",
        &headers,
        &json!({
            "actor": &actor,
            "id": id,
            "ifMatch": expected_revision,
            "request": &request,
        }),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: ContentEntry = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if request.reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "Rollback reason must contain at least 10 characters.",
        ));
    }
    if before.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The content entry changed; reload before rolling back.",
        ));
    }
    let historic = state
        .load_content_revision(id, request.revision)
        .await?
        .ok_or_else(|| ApiError::not_found("The requested historical revision was not found."))?;

    // Rollback means re-publishing historic content as a new immutable
    // revision. The original history is never modified.
    let mut restored = historic;
    restored.current_revision = before.current_revision + 1;
    restored.published_revision = Some(restored.current_revision);
    restored.status = PublicationStatus::Published;
    restored.scheduled_for = None;
    restored.updated_at = Utc::now();
    if restored.is_placeholder {
        restored.seo.indexable = false;
    }
    state
        .publish_content_projection(&restored, &actor, "rollback", expected_revision)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .content
            .insert(id, restored.clone());
    }
    let rollback_reason = request.reason.clone();
    audit(
        &state,
        &headers,
        &actor,
        "content.rollback",
        "content",
        Some(id),
        Some(json!(before)),
        Some(json!({"published": restored, "sourceRevision": request.revision, "reason": request.reason})),
        Some(rollback_reason),
    )
    .await?;
    idempotency
        .complete(&state, &restored, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &restored,
        restored.current_revision,
    ))
}

async fn list_products(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<Product>>, ApiError> {
    let mut values = state.list_working_products().await?;
    values.sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
    Ok(Json(paginate_by_id(
        "admin.products",
        values,
        query,
        |product| product.id,
    )?))
}

async fn publish_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected_revision = parse_if_match(&headers)?;
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.product.publish",
        &headers,
        &json!({"actor": &actor, "id": id, "ifMatch": expected_revision}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let product: Product = replay.decode()?;
            return Ok(entity_response(status, &product, product.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let product = state
        .load_working_product(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Product was not found in validated staging."))?;
    if product.current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The product changed; reload before publishing.",
        ));
    }
    state.assert_product_publishable(&product).await?;
    if product.status == PublicationStatus::Published
        && product.published_revision == Some(product.current_revision)
        && state
            .product_presentation_is_published(product.id, &product.locale)
            .await?
    {
        let response_product = state.present_product(product).await?;
        idempotency
            .complete(&state, &response_product, StatusCode::OK)
            .await?;
        return Ok(entity_response(
            StatusCode::OK,
            &response_product,
            response_product.current_revision,
        ));
    }
    let now = Utc::now();
    let mut published = product.clone();
    published.status = PublicationStatus::Published;
    published.published_revision = Some(published.current_revision);
    published.updated_at = now;
    state
        .publish_product_projection(&product, &published)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .products
            .insert(id, published.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "product.publish",
        "product",
        Some(id),
        Some(json!(product)),
        Some(json!(published)),
        Some("Publish validated Product Master revision".into()),
    )
    .await?;
    let response_product = state.present_product(published).await?;
    idempotency
        .complete(&state, &response_product, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &response_product,
        response_product.current_revision,
    ))
}

async fn list_temporary_overrides(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<TemporaryOverride>>, ApiError> {
    let mut values: Vec<_> = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT id,product_id,field_path,value,reason,created_at,expires_at
               FROM product_temporary_overrides WHERE product_id=$1
               ORDER BY created_at DESC,id"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let expires_at = row.try_get("expires_at")?;
                Ok(TemporaryOverride {
                    id: row.try_get("id")?,
                    product_id: row.try_get("product_id")?,
                    field_path: row.try_get("field_path")?,
                    value: row.try_get("value")?,
                    reason: row.try_get("reason")?,
                    created_at: row.try_get("created_at")?,
                    expires_at,
                    expired: expires_at <= Utc::now(),
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        state
            .data
            .read()
            .await
            .temporary_overrides
            .values()
            .filter(|value| value.product_id == id)
            .cloned()
            .collect()
    };
    values.sort_by_key(|entry| Reverse(entry.created_at));
    Ok(Json(paginate_by_id(
        "admin.productOverrides",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn create_temporary_override(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(mut input): Json<CreateTemporaryOverride>,
) -> Result<Response, ApiError> {
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.product.override.create",
        &headers,
        &json!({"actor": &actor, "id": id, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let value: TemporaryOverride = replay.decode()?;
            return Ok((status, Json(value)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if input.product_id != id {
        return Err(ApiError::bad_request(
            "Path product id must match productId.",
        ));
    }
    if state.load_working_product(id).await?.is_none() {
        return Err(ApiError::not_found("Product was not found."));
    }
    if input.reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "Override reason must contain at least 10 characters.",
        ));
    }
    if input.field_path.trim().is_empty() || input.field_path.starts_with("site.") {
        return Err(ApiError::bad_request(
            "fieldPath must identify a Feishu-owned source field; site-owned fields do not require overrides.",
        ));
    }
    if let Some(expires_at) = input.expires_at {
        if expires_at <= Utc::now() {
            return Err(ApiError::bad_request("expiresAt must be in the future."));
        }
    } else {
        let default_days = state
            .integer_setting("temporaryOverrideDefaultDays", 30, 1, 365)
            .await?;
        input.expires_at = Some(Utc::now() + ChronoDuration::days(default_days));
    }
    input.reason = input.reason.trim().to_owned();
    let value = TemporaryOverride::from_input(input);
    state.persist_override(&value).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .temporary_overrides
            .insert(value.id, value.clone());
    }
    let override_reason = value.reason.clone();
    audit(
        &state,
        &headers,
        &actor,
        "product.override.create",
        "temporaryOverride",
        Some(value.id),
        None,
        Some(json!(value)),
        Some(override_reason),
    )
    .await?;
    idempotency
        .complete(&state, &value, StatusCode::CREATED)
        .await?;
    Ok((StatusCode::CREATED, Json(value)).into_response())
}

async fn list_sync_runs(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<SyncRun>>, ApiError> {
    let values = state.list_sync_runs().await?;
    Ok(Json(paginate_by_id(
        "admin.feishuSyncRuns",
        values,
        query,
        |run| run.id,
    )?))
}

async fn start_sync_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StartSyncRequest>,
) -> Result<Response, ApiError> {
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.feishu.sync.start",
        &headers,
        &json!({"actor": &actor, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let run: SyncRun = replay.decode()?;
            return Ok((status, Json(run)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if request.mapping_version.trim().is_empty() {
        return Err(ApiError::bad_request("mappingVersion is required."));
    }
    let adapter_unavailable = state.pool.is_none();
    let now = Utc::now();
    let run = SyncRun {
        id: Uuid::new_v4(),
        source: "feishu".into(),
        dry_run: request.dry_run,
        mapping_version: request.mapping_version,
        status: if adapter_unavailable {
            SyncRunStatus::Failed
        } else {
            SyncRunStatus::Queued
        },
        resume_cursor: request.cursor,
        records_seen: 0,
        records_valid: 0,
        conflict_count: 0,
        started_at: now,
        completed_at: adapter_unavailable.then_some(now),
        error: adapter_unavailable.then(|| {
            "Feishu synchronization requires PostgreSQL and a configured provider adapter; no synchronization ran."
                .into()
        }),
    };
    state.enqueue_sync_run(&run).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .sync_runs
            .insert(run.id, run.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "feishu.sync.queue",
        "syncRun",
        Some(run.id),
        None,
        Some(json!(run)),
        Some("Queue Feishu staging synchronization".into()),
    )
    .await?;
    idempotency
        .complete(&state, &run, StatusCode::ACCEPTED)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(run)).into_response())
}

async fn list_conflicts(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::SyncConflict>>, ApiError> {
    let mut values: Vec<_> = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT id,sync_run_id,product_id,source_record_id,field_diffs,resolved_at,resolution
               FROM sync_conflicts ORDER BY id"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(crate::models::SyncConflict {
                    id: row.try_get("id")?,
                    sync_run_id: row.try_get("sync_run_id")?,
                    product_id: row.try_get("product_id")?,
                    source_record_id: row.try_get("source_record_id")?,
                    diffs: serde_json::from_value(row.try_get("field_diffs")?).map_err(|_| {
                        ApiError::service_unavailable("Stored sync conflict data is invalid.")
                    })?,
                    resolved_at: row.try_get("resolved_at")?,
                    resolution: row.try_get("resolution")?,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        state
            .data
            .read()
            .await
            .conflicts
            .values()
            .cloned()
            .collect()
    };
    values.sort_by_key(|entry| entry.id);
    Ok(Json(paginate_by_id(
        "admin.feishuConflicts",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn list_rfqs(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::RfqSubmission>>, ApiError> {
    let mut values = state.list_stored_rfqs().await?;
    values.sort_by_key(|submission| Reverse(submission.submitted_at));
    let mut page = paginate_by_id("admin.rfqs", values, query, |entry| entry.id)?;
    if !principal.has_permission("rfq.read_pii") {
        for value in &mut page.items {
            redact_business_contact(&mut value.request.contact);
        }
    }
    Ok(Json(page))
}

async fn list_contacts(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::ContactRequest>>, ApiError> {
    let mut values = state.list_stored_contacts().await?;
    values.sort_by_key(|contact| Reverse(contact.submitted_at));
    let mut page = paginate_by_id("admin.contacts", values, query, |entry| entry.id)?;
    if !principal.has_permission("rfq.read_pii") {
        for value in &mut page.items {
            redact_business_contact(&mut value.request.contact);
            value.request.message = "[restricted]".into();
        }
    }
    Ok(Json(page))
}

fn redact_business_contact(contact: &mut crate::models::BusinessContact) {
    contact.name = "[restricted]".into();
    contact.email = "[restricted]".into();
    contact.phone = None;
}

async fn analytics_summary(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    if let Some(pool) = &state.pool {
        let accepted_event_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM analytics_events")
                .fetch_one(pool)
                .await?;
        let rfq_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM rfq_submissions")
            .fetch_one(pool)
            .await?;
        let contact_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM contact_requests")
            .fetch_one(pool)
            .await?;
        return Ok(Json(json!({
            "acceptedEventCount": accepted_event_count,
            "rfqCount": rfq_count,
            "contactCount": contact_count,
            "containsPii": false,
            "source": "firstParty"
        })));
    }
    let data = state.data.read().await;
    Ok(Json(json!({
        "acceptedEventCount": data.analytics_receipts.values().filter(|event| event.accepted).count(),
        "rfqCount": data.rfqs.len(),
        "contactCount": data.contacts.len(),
        "containsPii": false,
        "source": "firstParty"
    })))
}

async fn list_operations(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<BackgroundOperation>>, ApiError> {
    let values = state.list_operations().await?;
    Ok(Json(paginate_by_id(
        "admin.operations",
        values,
        query,
        |operation| operation.id,
    )?))
}

async fn create_operation(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<CreateOperationRequest>,
) -> Result<Response, ApiError> {
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.operation.create",
        &headers,
        &json!({"actor": &actor, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let operation: BackgroundOperation = replay.decode()?;
            return Ok(operation_response(status, &operation));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if request.reason.trim().len() < 10 {
        return Err(ApiError::bad_request(
            "Operation reason must contain at least 10 characters.",
        ));
    }
    let expected = confirmation_phrase(request.kind);
    if request.confirmation != expected {
        return Err(ApiError::bad_request(format!(
            "confirmation must exactly equal `{expected}`."
        )));
    }
    require_operation_totp(&state, &headers, &principal, request.kind).await?;
    let now = Utc::now();
    let operation = BackgroundOperation {
        id: Uuid::new_v4(),
        kind: request.kind,
        status: OperationStatus::Queued,
        reason: request.reason.trim().into(),
        created_at: now,
        updated_at: now,
        result: None,
    };
    state.persist_operation(&operation).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .operations
            .insert(operation.id, operation.clone());
    }
    let operation_reason = operation.reason.clone();
    audit(
        &state,
        &headers,
        &actor,
        "operation.queue",
        "operation",
        Some(operation.id),
        None,
        Some(json!(operation)),
        Some(operation_reason),
    )
    .await?;
    idempotency
        .complete(&state, &operation, StatusCode::ACCEPTED)
        .await?;
    Ok(operation_response(StatusCode::ACCEPTED, &operation))
}

fn operation_response(status: StatusCode, operation: &BackgroundOperation) -> Response {
    let mut response = (status, Json(operation)).into_response();
    response.headers_mut().insert(
        "location",
        HeaderValue::from_str(&format!("/api/admin/v1/operations/{}", operation.id))
            .expect("operation location is valid"),
    );
    response
}

async fn require_operation_totp(
    state: &AppState,
    headers: &HeaderMap,
    principal: &AdminPrincipal,
    kind: OperationKind,
) -> Result<(), ApiError> {
    if !matches!(
        kind,
        OperationKind::MigrationApply
            | OperationKind::Backup
            | OperationKind::RestoreValidate
            | OperationKind::RetentionApply
    ) {
        return Ok(());
    }
    if !principal
        .role
        .split(" · ")
        .any(|role| role == "Super Admin")
    {
        return Err(ApiError::forbidden(
            "A Super Admin role is required for high-risk operations.",
        ));
    }
    let code = headers
        .get("x-totp-code")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() == 6 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| ApiError::forbidden("A valid X-TOTP-Code is required."))?;
    crate::auth::verify_totp_reauthentication(state, principal, code).await
}

async fn get_operation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<BackgroundOperation>, ApiError> {
    state
        .get_operation(id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("Operation was not found."))
}

async fn operation_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let operation = state
        .get_operation(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Operation was not found."))?;
    let updates = stream::unfold(
        (state, id, Some(operation), false),
        |(state, id, initial, finished)| async move {
            if finished {
                return None;
            }
            let operation = match initial {
                Some(operation) => Some(operation),
                None => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    state.get_operation(id).await.ok().flatten()
                }
            };
            let Some(operation) = operation else {
                let event = Event::default()
                    .event("error")
                    .data("Operation status is temporarily unavailable.");
                return Some((Ok(event), (state, id, None, true)));
            };
            let finished = matches!(
                operation.status,
                OperationStatus::Completed | OperationStatus::Failed
            );
            let event = Event::default()
                .event("operation")
                .json_data(&operation)
                .unwrap_or_else(|_| {
                    Event::default()
                        .event("error")
                        .data("Operation status serialization failed.")
                });
            Some((Ok(event), (state, id, None, finished)))
        },
    );
    Ok(Sse::new(updates).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("operation-stream"),
    ))
}

async fn list_audit(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<AuditEvent>>, ApiError> {
    let mut values = state.list_stored_audit().await?;
    values.sort_by_key(|event| Reverse(event.occurred_at));
    Ok(Json(paginate_by_id(
        "admin.audit",
        values,
        query,
        |event| event.id,
    )?))
}

const DEDICATED_NEWS_API_DETAIL: &str =
    "News must be managed through the dedicated /api/admin/v1/news API.";

fn reject_generic_news_input(kind: ContentKind) -> Result<(), ApiError> {
    if kind == ContentKind::News {
        return Err(ApiError::validation(BTreeMap::from([(
            "kind".into(),
            vec![DEDICATED_NEWS_API_DETAIL.into()],
        )])));
    }
    Ok(())
}

fn reject_generic_news_entry(entry: &ContentEntry) -> Result<(), ApiError> {
    if entry.kind == ContentKind::News {
        return Err(ApiError::conflict(DEDICATED_NEWS_API_DETAIL));
    }
    Ok(())
}

pub(super) fn validate_content_input(input: &ContentDraftInput) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if input.title.trim().is_empty() || input.title.len() > 300 {
        errors.insert(
            "title".into(),
            vec!["Title must contain 1 to 300 characters.".into()],
        );
    }
    if !valid_slug(&input.slug) {
        errors.insert(
            "slug".into(),
            vec!["Slug must contain lowercase letters, digits and single hyphens.".into()],
        );
    }
    if input.locale != "en" {
        errors.insert(
            "locale".into(),
            vec!["Only the launch locale `en` is enabled.".into()],
        );
    }
    if input.body.schema_version != 1 {
        errors.insert(
            "body.schemaVersion".into(),
            vec!["Only schema version 1 is supported.".into()],
        );
    }
    if let Err(detail) = validate_rich_text_node(&input.body.doc) {
        errors.insert("body.doc".into(), vec![detail]);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_rich_text_node(value: &Value) -> Result<(), String> {
    const ALLOWED_TYPES: &[&str] = &[
        "doc",
        "paragraph",
        "text",
        "heading",
        "bulletList",
        "orderedList",
        "listItem",
        "blockquote",
        "callout",
        "hardBreak",
        "image",
        "gallery",
        "table",
        "tableRow",
        "tableCell",
        "tableHeader",
        "codeBlock",
        "mathBlock",
        "cta",
        "relatedContent",
        "entityBlock",
        "bold",
        "italic",
        "strike",
        "underline",
        "code",
        "link",
    ];
    match value {
        Value::Object(map) => {
            if map.keys().any(|key| {
                matches!(
                    key.to_ascii_lowercase().as_str(),
                    "html" | "rawhtml" | "script" | "style" | "iframe"
                )
            }) {
                return Err("Raw HTML, script, style and iframe fields are not allowed.".into());
            }
            if let Some(node_type) = map.get("type").and_then(Value::as_str) {
                if !ALLOWED_TYPES.contains(&node_type) {
                    return Err(format!("Node or mark type `{node_type}` is not allowed."));
                }
                if node_type == "entityBlock" {
                    const ENTITY_KINDS: &[&str] = &[
                        "media",
                        "cta",
                        "related-product",
                        "related-content",
                        "formula",
                    ];
                    let kind = map
                        .get("attrs")
                        .and_then(Value::as_object)
                        .and_then(|attrs| attrs.get("kind"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| "entityBlock requires a string attrs.kind.".to_owned())?;
                    if !ENTITY_KINDS.contains(&kind) {
                        return Err(format!("entityBlock attrs.kind `{kind}` is not allowed."));
                    }
                }
            }
            for child in map.values() {
                validate_rich_text_node(child)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                validate_rich_text_node(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 180
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn confirmation_phrase(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::MigrationPreflight => "PREFLIGHT MIGRATION",
        OperationKind::MigrationApply => "APPLY MIGRATION",
        OperationKind::Backup => "CREATE BACKUP",
        OperationKind::RestoreValidate => "VALIDATE RESTORE",
        OperationKind::RetentionApply => "APPLY RETENTION",
        OperationKind::SearchReindex => "REBUILD SEARCH INDEX",
        OperationKind::CacheInvalidate => "INVALIDATE PUBLIC CACHE",
        OperationKind::FeishuSync => "START FEISHU SYNC",
        OperationKind::ProductImport => "IMPORT PRODUCT MASTER",
    }
}

fn entity_response<T: serde::Serialize>(status: StatusCode, value: &T, revision: i64) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response
}

fn add_private_no_store_headers(response: &mut Response) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}

// Keeping these fields explicit at the mutation call sites makes every audit
// record reviewable there (actor, request, entity, before/after and reason)
// instead of hiding security-relevant values behind a partially filled map.
#[allow(clippy::too_many_arguments)]
async fn audit(
    state: &AppState,
    headers: &HeaderMap,
    actor: &str,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> Result<(), ApiError> {
    state
        .persist_audit(AuditEvent {
            id: Uuid::new_v4(),
            actor: actor.into(),
            action: action.into(),
            entity_type: entity_type.into(),
            entity_id,
            before,
            after,
            reason,
            request_id: headers
                .get("x-request-id")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| Uuid::parse_str(value).ok())
                .unwrap_or_else(Uuid::new_v4),
            occurred_at: Utc::now(),
        })
        .await
}
