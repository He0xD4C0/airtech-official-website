use std::collections::{BTreeMap, HashSet};

use axum::{
    extract::{DefaultBodyLimit, Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use ring::{
    aead::{self, Aad, LessSafeKey, Nonce, UnboundKey},
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    auth::AdminPrincipal,
    config::InvitationReplayEncryptionKey,
    error::{json_hash, ApiError},
    idempotency::{
        begin as begin_idempotency, idempotency_key as parse_idempotency_key, IdempotencyOutcome,
    },
    models::{
        AdminProductDetail, AdminRoleRecord, AdminUserRecord, AuditEvent, ContentEntry,
        ContentKind, CursorPage, DataClass, GeneralInformation, GeneralInformationDraftInput,
        GuestSourceDaily, GuestVisitAggregate, InviteAdminUser, NewsDraftInput, NewsEntry,
        ProductImportRequest, ProductImportResult, ProductPresentation, ProductPrivatePricing,
        PublicationStatus, UpdateAdminRole, UpdateAdminUser, UpdateProductPresentation,
        UserInvitation,
    },
    pagination::{
        cursor_limit, decode_scoped_cursor, encode_scoped_cursor, paginate_by_id, CursorQuery,
    },
    routes::{actor, etag, parse_if_match},
    services::product_import::{
        load_product_import_result as load_stored_product_import, parse_product_master,
        stage_and_queue_product_import,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/news", get(list_news).post(create_news))
        .route("/news/{id}", get(get_news).patch(update_news))
        .route("/news/{id}/revisions", get(list_news_revisions))
        .route("/news/{id}/publish", post(publish_news))
        .route("/news/{id}/rollback", post(rollback_news))
        .route(
            "/general-information",
            get(get_general_information).post(create_general_information),
        )
        .route(
            "/general-information/{id}",
            get(get_general_information_by_id).patch(update_general_information),
        )
        .route(
            "/general-information/{id}/revisions",
            get(list_general_information_revisions),
        )
        .route(
            "/general-information/{id}/publish",
            post(publish_general_information),
        )
        .route(
            "/general-information/{id}/rollback",
            post(rollback_general_information),
        )
        .route(
            "/products/imports",
            get(list_product_imports)
                .post(import_products)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024 + 64 * 1024)),
        )
        .route("/products/imports/{id}", get(get_product_import))
        .route("/products/{id}", get(get_admin_product))
        .route("/products/{id}/private-pricing", get(get_private_pricing))
        .route(
            "/products/{id}/presentation",
            patch(update_product_presentation),
        )
        .route("/analytics/visits", get(list_guest_visits))
        .route("/analytics/sources", get(list_guest_sources))
        .route("/users", get(list_users))
        .route("/users/{id}", get(get_user).patch(update_user))
        .route("/users/{id}/sessions", delete(revoke_user_sessions))
        .route("/user-invitations", get(list_invitations).post(invite_user))
        .route("/user-invitations/{id}/revoke", post(revoke_invitation))
        .route("/roles", get(list_roles))
        .route("/roles/{id}", get(get_role).patch(update_role))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocaleQuery {
    #[serde(default = "default_locale")]
    locale: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevisionRequest {
    revision: i64,
    reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReasonRequest {
    reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncryptedInvitationReplay {
    version: u8,
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnalyticsQuery {
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
    cursor: Option<String>,
    limit: Option<usize>,
}

impl AnalyticsQuery {
    fn pagination(&self) -> CursorQuery {
        CursorQuery {
            cursor: self.cursor.clone(),
            limit: self.limit,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalyticsCursor {
    bucket_date: chrono::NaiveDate,
    dimension_hash: Vec<u8>,
}

// News is a revisioned ContentKind::News document with revision-specific list metadata.
async fn list_news(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<NewsEntry>>, ApiError> {
    let mut values = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT entry.payload, news.category, news.author_display_name,
                      news.cover_media_asset_id, news.publication_at, news.featured,
                      news.data_origin
               FROM content_entries entry
               JOIN news_working news ON news.content_id=entry.id
               WHERE entry.kind='news' ORDER BY entry.updated_at DESC"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(decode_news_row)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        state.data.read().await.news.values().cloned().collect()
    };
    values.sort_by_key(|entry| std::cmp::Reverse(entry.content.updated_at));
    Ok(Json(paginate_by_id(
        "admin.news",
        values,
        query,
        |entry| entry.content.id,
    )?))
}

async fn get_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let entry = load_news(&state, id).await?;
    Ok(entity_response(
        StatusCode::OK,
        &entry,
        entry.content.current_revision,
    ))
}

async fn list_news_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CursorPage<NewsEntry>>, ApiError> {
    // Resolve the working record first so an unknown id cannot be confused
    // with an entry that simply has no immutable snapshots yet.
    load_news(&state, id).await?;
    let mut revisions = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT content_revision.payload,news.category,news.author_display_name,
                      news.cover_media_asset_id,news.publication_at,news.featured,
                      news.data_origin
               FROM content_revisions content_revision
               JOIN content_entries entry ON entry.id=content_revision.content_id
               JOIN news ON news.content_id=content_revision.content_id
                 AND news.revision=content_revision.revision
               WHERE content_revision.content_id=$1 AND entry.kind='news'
               ORDER BY content_revision.revision DESC"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(decode_news_row)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let data = state.data.read().await;
        let metadata = data.news.get(&id).cloned();
        data.content_revisions
            .get(&id)
            .into_iter()
            .flat_map(|values| values.values())
            .filter_map(|content| {
                metadata.clone().map(|metadata| NewsEntry {
                    content: content.clone(),
                    ..metadata
                })
            })
            .collect()
    };
    revisions.sort_by_key(|entry| std::cmp::Reverse(entry.content.current_revision));
    Ok(Json(CursorPage::all(revisions)))
}

async fn create_news(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut input): Json<NewsDraftInput>,
) -> Result<Response, ApiError> {
    normalize_news_ownership(state.config.production, None, &mut input)?;
    validate_news_input(&mut input)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.news.create",
        &headers,
        &json!({"actor": &actor_name, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let now = Utc::now();
    let entry = NewsEntry {
        content: ContentEntry {
            id: Uuid::new_v4(),
            kind: ContentKind::News,
            slug: input.content.slug,
            locale: input.content.locale,
            title: input.content.title,
            summary: input.content.summary,
            body: input.content.body,
            seo: input.content.seo,
            status: PublicationStatus::Draft,
            is_placeholder: input.content.is_placeholder,
            current_revision: 1,
            published_revision: None,
            scheduled_for: None,
            updated_at: now,
        },
        category: input.category,
        author_display_name: Some(input.author_display_name),
        cover_media_id: input.cover_media_id,
        published_at: input.published_at,
        featured: input.featured,
        data_class: input.data_class,
    };
    let audit = mutation_audit_event(
        &headers,
        "news.create",
        "news",
        Some(entry.content.id),
        None,
        Some(json!(entry)),
        Some("Create news working draft".into()),
    );
    persist_news_revision(&state, &entry, None, false, &actor_name, &audit).await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content.insert(entry.content.id, entry.content.clone());
        data.news.insert(entry.content.id, entry.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &entry, StatusCode::CREATED)
        .await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &entry,
        entry.content.current_revision,
    ))
}

async fn update_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(mut input): Json<NewsDraftInput>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let before = load_news(&state, id).await?;
    normalize_news_ownership(state.config.production, Some(before.data_class), &mut input)?;
    validate_news_input(&mut input)?;
    let idempotency = match begin_idempotency(
        &state,
        "admin.news.update",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if before.content.current_revision != expected {
        return Err(ApiError::conflict(
            "The news entry changed; reload before saving.",
        ));
    }
    let mut updated = before.clone();
    updated.content.slug = input.content.slug;
    updated.content.locale = input.content.locale;
    updated.content.title = input.content.title;
    updated.content.summary = input.content.summary;
    updated.content.body = input.content.body;
    updated.content.seo = input.content.seo;
    updated.content.is_placeholder = input.content.is_placeholder;
    updated.content.status = PublicationStatus::Draft;
    updated.content.current_revision += 1;
    updated.content.updated_at = Utc::now();
    updated.category = input.category;
    updated.author_display_name = Some(input.author_display_name);
    updated.cover_media_id = input.cover_media_id;
    updated.published_at = input.published_at;
    updated.featured = input.featured;
    updated.data_class = input.data_class;
    let audit = mutation_audit_event(
        &headers,
        "news.update",
        "news",
        Some(id),
        Some(json!(before)),
        Some(json!(updated)),
        Some("Update news working draft".into()),
    );
    persist_news_revision(&state, &updated, Some(expected), false, &actor_name, &audit).await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content.insert(id, updated.content.clone());
        data.news.insert(id, updated.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &updated, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &updated,
        updated.content.current_revision,
    ))
}

async fn publish_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.news.publish",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_news(&state, id).await?;
    if before.content.current_revision != expected {
        return Err(ApiError::conflict(
            "The news entry changed; reload before publishing.",
        ));
    }
    let mut published = before.clone();
    published.content.status = PublicationStatus::Published;
    published.content.published_revision = Some(expected);
    published.content.updated_at = Utc::now();
    if published.content.is_placeholder || published.data_class == DataClass::DevelopmentFixture {
        published.content.seo.indexable = false;
    }
    let audit = mutation_audit_event(
        &headers,
        "news.publish",
        "news",
        Some(id),
        Some(json!(before)),
        Some(json!(published)),
        Some("Publish current news revision".into()),
    );
    persist_news_revision(
        &state,
        &published,
        Some(expected),
        true,
        &actor_name,
        &audit,
    )
    .await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content_revisions
            .entry(id)
            .or_default()
            .insert(expected, published.content.clone());
        data.content.insert(id, published.content.clone());
        data.published_content.insert(id, published.content.clone());
        data.news.insert(id, published.clone());
        data.published_news.insert(id, published.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &published, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &published,
        published.content.current_revision,
    ))
}

async fn rollback_news(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RevisionRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.news.rollback",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: NewsEntry = replay.decode()?;
            return Ok(entity_response(
                status,
                &entry,
                entry.content.current_revision,
            ));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_news(&state, id).await?;
    if before.content.current_revision != expected {
        return Err(ApiError::conflict(
            "The news entry changed; reload before rolling back.",
        ));
    }
    let mut restored = load_news_revision(&state, id, request.revision).await?;
    // Fixture ownership is one-way. Republishing a historic fixture snapshot
    // after editorial takeover must not hand the deterministic record back to
    // the development seed.
    if before.data_class == DataClass::Editorial || state.config.production {
        restored.data_class = DataClass::Editorial;
    }
    restored.content.current_revision = expected + 1;
    restored.content.published_revision = Some(expected + 1);
    restored.content.status = PublicationStatus::Published;
    restored.content.updated_at = Utc::now();
    if restored.content.is_placeholder || restored.data_class == DataClass::DevelopmentFixture {
        restored.content.seo.indexable = false;
    }
    let audit = mutation_audit_event(
        &headers,
        "news.rollback",
        "news",
        Some(id),
        Some(json!(before)),
        Some(json!(restored)),
        Some(request.reason.clone()),
    );
    persist_news_revision(&state, &restored, Some(expected), true, &actor_name, &audit).await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.content_revisions
            .entry(id)
            .or_default()
            .insert(restored.content.current_revision, restored.content.clone());
        data.content.insert(id, restored.content.clone());
        data.published_content.insert(id, restored.content.clone());
        data.news.insert(id, restored.clone());
        data.published_news.insert(id, restored.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &restored, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &restored,
        restored.content.current_revision,
    ))
}

// General Information is one locale-aware revisioned site aggregate.
async fn get_general_information(
    State(state): State<AppState>,
    Query(query): Query<LocaleQuery>,
) -> Result<Response, ApiError> {
    let entry = load_general_information_by_locale(&state, &query.locale)
        .await?
        .ok_or_else(|| ApiError::not_found("General Information was not found."))?;
    Ok(entity_response(
        StatusCode::OK,
        &entry,
        entry.current_revision,
    ))
}

async fn get_general_information_by_id(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let entry = load_general_information(&state, id).await?;
    Ok(entity_response(
        StatusCode::OK,
        &entry,
        entry.current_revision,
    ))
}

async fn list_general_information_revisions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CursorPage<GeneralInformation>>, ApiError> {
    let current = load_general_information(&state, id).await?;
    let mut revisions = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT information.id,revision.locale,revision.payload,
                      information.published_revision,revision.is_placeholder,
                      revision.revision,revision.created_at
               FROM general_information_revisions revision
               JOIN general_information information
                 ON information.id=revision.general_information_id
               WHERE revision.general_information_id=$1
               ORDER BY revision.revision DESC"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let revision: i64 = row.try_get("revision")?;
                let published_revision: Option<i64> = row.try_get("published_revision")?;
                Ok::<GeneralInformation, ApiError>(GeneralInformation {
                    id: row.try_get("id")?,
                    locale: row.try_get("locale")?,
                    payload: row.try_get("payload")?,
                    status: if published_revision == Some(revision) {
                        PublicationStatus::Published
                    } else {
                        PublicationStatus::Draft
                    },
                    current_revision: revision,
                    published_revision: (published_revision == Some(revision)).then_some(revision),
                    is_placeholder: row.try_get("is_placeholder")?,
                    updated_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        state
            .data
            .read()
            .await
            .general_information_revisions
            .get(&id)
            .map(|values| values.values().cloned().collect())
            .unwrap_or_default()
    };
    revisions.sort_by_key(|entry| std::cmp::Reverse(entry.current_revision));
    // Retain the current read above so future query changes do not accidentally
    // turn this endpoint into a cross-tenant revision lookup.
    debug_assert_eq!(current.id, id);
    Ok(Json(CursorPage::all(revisions)))
}

async fn create_general_information(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<GeneralInformationDraftInput>,
) -> Result<Response, ApiError> {
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.create",
        &headers,
        &json!({"actor": &actor_name, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_general_information(&input)?;
    if load_general_information_by_locale(&state, &input.locale)
        .await?
        .is_some()
    {
        return Err(ApiError::conflict(
            "General Information already exists for this locale.",
        ));
    }
    let entry = GeneralInformation {
        id: Uuid::new_v4(),
        locale: input.locale,
        payload: input.payload,
        status: PublicationStatus::Draft,
        current_revision: 1,
        published_revision: None,
        is_placeholder: input.is_placeholder,
        updated_at: Utc::now(),
    };
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.create",
        "generalInformation",
        Some(entry.id),
        None,
        Some(json!(entry)),
        Some("Create General Information draft".into()),
    );
    persist_general_information_revision(&state, &entry, None, false, &actor_name, &audit, None)
        .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .general_information
            .insert(entry.id, entry.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &entry, StatusCode::CREATED)
        .await?;
    Ok(entity_response(
        StatusCode::CREATED,
        &entry,
        entry.current_revision,
    ))
}

async fn update_general_information(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<GeneralInformationDraftInput>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.update",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_general_information(&input)?;
    let before = load_general_information(&state, id).await?;
    if input.locale != before.locale {
        return Err(ApiError::validation(BTreeMap::from([(
            "locale".into(),
            vec!["Locale is immutable after General Information is created.".into()],
        )])));
    }
    if before.current_revision != expected {
        return Err(ApiError::conflict(
            "General Information changed; reload before saving.",
        ));
    }
    let mut updated = before.clone();
    updated.locale = input.locale;
    updated.payload = input.payload;
    updated.is_placeholder = input.is_placeholder;
    updated.status = PublicationStatus::Draft;
    updated.current_revision += 1;
    updated.updated_at = Utc::now();
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.update",
        "generalInformation",
        Some(id),
        Some(json!(before)),
        Some(json!(updated)),
        Some("Update General Information draft".into()),
    );
    persist_general_information_revision(
        &state,
        &updated,
        Some(expected),
        false,
        &actor_name,
        &audit,
        None,
    )
    .await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .general_information
            .insert(id, updated.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &updated, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &updated,
        updated.current_revision,
    ))
}

async fn publish_general_information(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.publish",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_general_information(&state, id).await?;
    if before.current_revision != expected {
        return Err(ApiError::conflict(
            "General Information changed; reload before publishing.",
        ));
    }
    let mut published = before.clone();
    published.status = PublicationStatus::Published;
    published.published_revision = Some(expected);
    published.updated_at = Utc::now();
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.publish",
        "generalInformation",
        Some(id),
        Some(json!(before)),
        Some(json!(published)),
        Some("Publish General Information revision".into()),
    );
    persist_general_information_revision(
        &state,
        &published,
        Some(expected),
        true,
        &actor_name,
        &audit,
        Some("publish"),
    )
    .await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.general_information_revisions
            .entry(id)
            .or_default()
            .insert(published.current_revision, published.clone());
        data.general_information.insert(id, published.clone());
        data.published_general_information
            .insert(id, published.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &published, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &published,
        published.current_revision,
    ))
}

async fn rollback_general_information(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RevisionRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.generalInformation.rollback",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let entry: GeneralInformation = replay.decode()?;
            return Ok(entity_response(status, &entry, entry.current_revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_general_information(&state, id).await?;
    if before.current_revision != expected {
        return Err(ApiError::conflict(
            "General Information changed; reload before rolling back.",
        ));
    }
    let mut restored = load_general_information_revision(&state, id, request.revision).await?;
    restored.current_revision = expected + 1;
    restored.published_revision = Some(expected + 1);
    restored.status = PublicationStatus::Published;
    restored.updated_at = Utc::now();
    let audit = mutation_audit_event(
        &headers,
        "generalInformation.rollback",
        "generalInformation",
        Some(id),
        Some(json!(before)),
        Some(json!(restored)),
        Some(request.reason.clone()),
    );
    persist_general_information_revision(
        &state,
        &restored,
        Some(expected),
        true,
        &actor_name,
        &audit,
        Some("rollback"),
    )
    .await?;
    if state.pool.is_none() {
        let mut data = state.data.write().await;
        data.general_information_revisions
            .entry(id)
            .or_default()
            .insert(restored.current_revision, restored.clone());
        data.general_information.insert(id, restored.clone());
        data.published_general_information
            .insert(id, restored.clone());
    }
    persist_memory_audit_if_needed(&state, audit).await?;
    idempotency
        .complete(&state, &restored, StatusCode::OK)
        .await?;
    Ok(entity_response(
        StatusCode::OK,
        &restored,
        restored.current_revision,
    ))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductImportAccepted {
    operation_id: Uuid,
    status: String,
    operation_url: String,
    events_url: String,
}

async fn import_products(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<ProductImportRequest>,
) -> Result<Response, ApiError> {
    if request.csv.is_empty() || request.csv.len() > 16 * 1024 * 1024 {
        return Err(ApiError::bad_request(
            "csv must contain between 1 byte and 16 MiB.",
        ));
    }
    let idempotency =
        match begin_idempotency(&state, "admin.productImport", &headers, &request).await? {
            IdempotencyOutcome::Replay(replay) => {
                let status = replay.status()?;
                let accepted: ProductImportAccepted = replay.decode()?;
                return Ok((status, Json(accepted)).into_response());
            }
            IdempotencyOutcome::Fresh(context) => context,
        };
    let mapping_version = request
        .mapping_version
        .unwrap_or_else(|| state.config.product_import_mapping_version.clone());
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Product Master import requires PostgreSQL persistence.")
    })?;
    let actor_id = principal.user_id;
    let key = state.config.product_staging_encryption_key.clone();
    let parsed = tokio::task::spawn_blocking(move || {
        parse_product_master(&request.csv, &mapping_version, key.as_ref())
    })
    .await
    .map_err(|_| ApiError::internal("Product import validation task failed."))??;
    let staged = stage_and_queue_product_import(
        pool,
        parsed,
        state.environment_label(),
        state.config.approved_product_master.as_ref(),
        Some(actor_id),
        &actor(&headers),
    )
    .await?;
    let accepted = ProductImportAccepted {
        operation_id: staged.operation_id,
        status: if staged.queued {
            "queued".into()
        } else {
            staged.result.status.clone()
        },
        operation_url: format!("/api/admin/v1/operations/{}", staged.operation_id),
        events_url: format!("/api/admin/v1/operations/{}/events", staged.operation_id),
    };
    idempotency
        .complete(&state, &accepted, StatusCode::ACCEPTED)
        .await?;
    let mut response = (StatusCode::ACCEPTED, Json(accepted)).into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&format!("/api/admin/v1/operations/{}", staged.operation_id))
            .expect("operation URL is valid"),
    );
    Ok(response)
}

async fn list_product_imports(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<ProductImportResult>>, ApiError> {
    let mut values = if let Some(pool) = &state.pool {
        load_product_imports(pool).await?
    } else {
        state
            .data
            .read()
            .await
            .product_imports
            .values()
            .cloned()
            .collect()
    };
    values.sort_by_key(|entry| std::cmp::Reverse(entry.created_at));
    Ok(Json(paginate_by_id(
        "admin.productImports",
        values,
        query,
        |entry| entry.id,
    )?))
}

async fn load_product_imports(pool: &sqlx::PgPool) -> Result<Vec<ProductImportResult>, ApiError> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM product_import_runs ORDER BY created_at DESC,id",
    )
    .fetch_all(pool)
    .await?;
    let mut results = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(result) = load_stored_product_import(pool, id).await? {
            results.push(result);
        }
    }
    Ok(results)
}

async fn get_product_import(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProductImportResult>, ApiError> {
    let entry = if let Some(pool) = &state.pool {
        load_stored_product_import(pool, id).await?
    } else {
        state.data.read().await.product_imports.get(&id).cloned()
    }
    .ok_or_else(|| ApiError::not_found("Product import was not found."))?;
    Ok(Json(entry))
}

async fn get_admin_product(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let detail = load_admin_product_detail(&state, id).await?;
    let presentation_revision = detail
        .presentation
        .as_ref()
        .map(|presentation| presentation.revision)
        .unwrap_or_default();
    Ok(entity_response(
        StatusCode::OK,
        &detail,
        presentation_revision,
    ))
}

async fn get_private_pricing(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Private Product Master pricing requires PostgreSQL.")
    })?;
    let key = state
        .config
        .product_staging_encryption_key
        .as_ref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Product staging encryption is not configured; pricing cannot be decrypted.",
            )
        })?;
    let row = sqlx::query(
        r#"SELECT product.stable_id,staging.source_row_number,staging.nonce,
                  staging.ciphertext,staging.authentication_tag,
                  import_run.mapping_version,import_run.source_checksum
           FROM products product
           JOIN product_import_private_staging staging
             ON staging.import_run_id=product.product_import_run_id
            AND staging.source_record_id=product.stable_id
           JOIN product_import_runs import_run ON import_run.id=staging.import_run_id
           WHERE product.id=$1 AND product.data_origin='verifiedCsv'
             AND staging.status IN ('validated','promoted')
             AND staging.expires_at > now()
           ORDER BY staging.created_at DESC LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Private pricing was not found for this product."))?;
    let stable_id: String = row.try_get("stable_id")?;
    let source_row_number: i32 = row.try_get("source_row_number")?;
    let nonce: Vec<u8> = row.try_get("nonce")?;
    let ciphertext: Vec<u8> = row.try_get("ciphertext")?;
    let authentication_tag: Vec<u8> = row.try_get("authentication_tag")?;
    let mapping_version: String = row.try_get("mapping_version")?;
    let checksum: String = row.try_get("source_checksum")?;
    let pricing_fields = crate::services::product_import::decrypt_private_pricing(
        key,
        crate::services::product_import::PrivatePricingEnvelope {
            mapping_version: &mapping_version,
            checksum: &checksum,
            source_row_number,
            stable_id: &stable_id,
            nonce: &nonce,
            ciphertext: &ciphertext,
            authentication_tag: &authentication_tag,
        },
    )?;
    let result = ProductPrivatePricing {
        product_id: id,
        stable_id,
        source_row_number,
        pricing_fields,
    };
    audit_mutation(
        &state,
        &headers,
        "product.privatePricing.read",
        "product",
        Some(id),
        None,
        Some(json!({
            "sourceRowNumber": result.source_row_number,
            "pricingFieldCount": result.pricing_fields.len()
        })),
        Some("Read encrypted Product Master pricing fields".into()),
    )
    .await?;
    let mut response = (StatusCode::OK, Json(result)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}

async fn update_product_presentation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<UpdateProductPresentation>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let actor_name = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.productPresentation.update",
        &headers,
        &json!({"actor": &actor_name, "id": id, "ifMatch": expected, "input": &input}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let detail: AdminProductDetail = replay.decode()?;
            let revision = detail
                .presentation
                .as_ref()
                .map(|presentation| presentation.revision)
                .unwrap_or_default();
            return Ok(entity_response(status, &detail, revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_product_presentation(&input)?;
    let before = load_admin_product_detail(&state, id).await?;
    validate_product_canonical(&input, before.product.family)?;
    let now = Utc::now();
    let after = if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        let product_row = sqlx::query(
            r#"SELECT current_revision,data_origin,family FROM products
               WHERE id=$1 FOR UPDATE"#,
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| ApiError::not_found("Product was not found."))?;
        if product_row.try_get::<i64, _>("current_revision")? != before.product.current_revision {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The Product Master facts changed; reload before saving presentation fields.",
            ));
        }
        let existing = sqlx::query(
            r#"SELECT current_revision,published_revision
               FROM product_presentation_working
               WHERE product_id=$1 AND locale=$2 FOR UPDATE"#,
        )
        .bind(id)
        .bind(&input.locale)
        .fetch_optional(&mut *transaction)
        .await?;
        let stored_revision = existing
            .as_ref()
            .map(|row| row.try_get::<i64, _>("current_revision"))
            .transpose()?
            .unwrap_or_default();
        if stored_revision != expected {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The product presentation changed; reload before saving.",
            ));
        }
        let product_family: String = product_row.try_get("family")?;
        let route_key = format!(
            "product-presentation:{}:{}:{}",
            product_family, input.locale, input.slug
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(route_key)
            .execute(&mut *transaction)
            .await?;
        let slug_owner = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT presentation.product_id
               FROM product_presentation_working presentation
               JOIN products other_product ON other_product.id=presentation.product_id
               WHERE presentation.locale=$1 AND presentation.slug=$2
                 AND presentation.product_id<>$3 AND other_product.family=$4"#,
        )
        .bind(&input.locale)
        .bind(&input.slug)
        .bind(id)
        .bind(&product_family)
        .fetch_optional(&mut *transaction)
        .await?;
        if slug_owner.is_some() {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "Another product in this family already uses this locale and slug.",
            ));
        }
        let revision = expected + 1;
        if !input.related_content_ids.is_empty() {
            let related_count = sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM content_entries WHERE id=ANY($1)",
            )
            .bind(&input.related_content_ids)
            .fetch_one(&mut *transaction)
            .await?;
            if related_count != input.related_content_ids.len() as i64 {
                transaction.rollback().await?;
                return Err(ApiError::validation(BTreeMap::from([(
                    "relatedContentIds".into(),
                    vec!["Every related content id must exist.".into()],
                )])));
            }
        }
        let presentation_content = json!({
            "sortOrder": input.sort_order,
            "relatedContentIds": input.related_content_ids,
        });
        let presentation_seo = serde_json::to_value(&input.seo)
            .map_err(|_| ApiError::internal("Product presentation SEO serialization failed."))?;
        let product_data_origin: String = product_row.try_get("data_origin")?;
        let is_development_fixture = product_data_origin == "developmentFixture";
        let presentation_indexable = input.indexable && !is_development_fixture;
        let presentation_data_origin = if is_development_fixture {
            "developmentFixture"
        } else {
            "editorial"
        };
        let published_revision = existing
            .as_ref()
            .map(|row| row.try_get::<Option<i64>, _>("published_revision"))
            .transpose()?
            .flatten();
        if existing.is_some() {
            let updated = sqlx::query(
                r#"UPDATE product_presentation_working
                   SET current_revision=$3,slug=$4,title=$5,summary=$6,content=$7,
                       seo_metadata=$8,translation_state='draft',is_placeholder=$9,
                       indexable=$10,data_origin=$11,updated_by=$12,updated_at=$13
                   WHERE product_id=$1 AND locale=$2 AND current_revision=$14"#,
            )
            .bind(id)
            .bind(&input.locale)
            .bind(revision)
            .bind(&input.slug)
            .bind(&input.title)
            .bind(&input.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(is_development_fixture)
            .bind(presentation_indexable)
            .bind(presentation_data_origin)
            .bind(&actor_name)
            .bind(now)
            .bind(expected)
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The product presentation changed; reload before saving.",
                ));
            }
        } else {
            sqlx::query(
                r#"INSERT INTO product_presentation_working
                   (product_id,locale,current_revision,published_revision,slug,title,summary,
                    content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                    updated_by,updated_at)
                   VALUES ($1,$2,$3,NULL,$4,$5,$6,$7,$8,'draft',$9,$10,$11,$12,$13)"#,
            )
            .bind(id)
            .bind(&input.locale)
            .bind(revision)
            .bind(&input.slug)
            .bind(&input.title)
            .bind(&input.summary)
            .bind(&presentation_content)
            .bind(&presentation_seo)
            .bind(is_development_fixture)
            .bind(presentation_indexable)
            .bind(presentation_data_origin)
            .bind(&actor_name)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            r#"INSERT INTO product_presentation_revisions
               (product_id,locale,revision,source_product_revision,slug,title,summary,
                content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                created_by,created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'draft',$10,$11,$12,$13,$14)"#,
        )
        .bind(id)
        .bind(&input.locale)
        .bind(revision)
        .bind(before.product.current_revision)
        .bind(&input.slug)
        .bind(&input.title)
        .bind(&input.summary)
        .bind(&presentation_content)
        .bind(&presentation_seo)
        .bind(is_development_fixture)
        .bind(presentation_indexable)
        .bind(presentation_data_origin)
        .bind(&actor_name)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let presentation = ProductPresentation {
            locale: input.locale.clone(),
            slug: input.slug.clone(),
            title: input.title.clone(),
            summary: input.summary.clone(),
            seo: input.seo.clone(),
            indexable: presentation_indexable,
            sort_order: input.sort_order,
            related_content_ids: input.related_content_ids.clone(),
            revision,
            published_revision,
            updated_at: now,
        };
        let mut after = before.clone();
        overlay_product_presentation(&mut after.product, &presentation);
        after.presentation = Some(presentation);
        let audit = mutation_audit_event(
            &headers,
            "product.presentation.update",
            "productPresentation",
            Some(id),
            Some(json!(&before)),
            Some(json!(&after)),
            Some(input.reason.clone()),
        );
        insert_audit_event_in_transaction(&mut transaction, &audit).await?;
        let staged = idempotency
            .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
            .await?;
        transaction.commit().await?;
        staged.finish().await?;
        after
    } else {
        let mut data = state.data.write().await;
        let current_presentation = data
            .product_presentations
            .get(&(id, input.locale.clone()))
            .cloned()
            .or_else(|| before.presentation.clone());
        let stored_revision = current_presentation
            .as_ref()
            .map(|presentation| presentation.revision)
            .unwrap_or_default();
        if stored_revision != expected {
            return Err(ApiError::conflict(
                "The product presentation changed; reload before saving.",
            ));
        }
        if data
            .product_presentations
            .iter()
            .any(|((product_id, locale), presentation)| {
                *product_id != id
                    && locale == &input.locale
                    && presentation.slug == input.slug
                    && data
                        .products
                        .get(product_id)
                        .is_some_and(|other| other.family == before.product.family)
            })
        {
            return Err(ApiError::conflict(
                "Another product in this family already uses this locale and slug.",
            ));
        }
        if input
            .related_content_ids
            .iter()
            .any(|content_id| !data.content.contains_key(content_id))
        {
            return Err(ApiError::validation(BTreeMap::from([(
                "relatedContentIds".into(),
                vec!["Every related content id must exist.".into()],
            )])));
        }
        let presentation = ProductPresentation {
            locale: input.locale.clone(),
            slug: input.slug.clone(),
            title: input.title.clone(),
            summary: input.summary.clone(),
            seo: input.seo.clone(),
            indexable: input.indexable
                && !matches!(
                    &before.source_kind,
                    crate::models::DataClass::DevelopmentFixture
                ),
            sort_order: input.sort_order,
            related_content_ids: input.related_content_ids.clone(),
            revision: expected + 1,
            published_revision: current_presentation
                .and_then(|presentation| presentation.published_revision),
            updated_at: now,
        };
        data.product_presentations
            .insert((id, input.locale.clone()), presentation.clone());
        let mut after = before.clone();
        overlay_product_presentation(&mut after.product, &presentation);
        after.presentation = Some(presentation);
        drop(data);
        let audit = mutation_audit_event(
            &headers,
            "product.presentation.update",
            "productPresentation",
            Some(id),
            Some(json!(&before)),
            Some(json!(&after)),
            Some(input.reason),
        );
        persist_memory_audit_if_needed(&state, audit).await?;
        idempotency.complete(&state, &after, StatusCode::OK).await?;
        after
    };
    let revision = after
        .presentation
        .as_ref()
        .map(|presentation| presentation.revision)
        .unwrap_or_default();
    Ok(entity_response(StatusCode::OK, &after, revision))
}

async fn list_guest_visits(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<CursorPage<GuestVisitAggregate>>, ApiError> {
    validate_time_range(query.from, query.to)?;
    let scope = analytics_cursor_scope("admin.analytics.visits", query.from, query.to);
    let limit = cursor_limit(&query.pagination())?;
    let after = decode_analytics_cursor(&scope, query.cursor.as_deref())?;
    let values = if let Some(pool) = &state.pool {
        // Retention moves only expiring rows into guest_source_daily. Adding
        // the remaining raw rows therefore provides current reporting without
        // counting a visit or event on both sides of the transaction boundary.
        let rows = sqlx::query(
            r#"WITH metric_rows AS (
                   SELECT bucket_date,landing_path,locale,visits,page_views,
                          rfq_starts,rfq_submissions
                   FROM guest_source_daily
                   UNION ALL
                   SELECT (first_seen_at AT TIME ZONE 'UTC')::date AS bucket_date,
                          landing_path,locale,COUNT(*)::bigint AS visits,
                          0::bigint AS page_views,0::bigint AS rfq_starts,
                          0::bigint AS rfq_submissions
                   FROM guest_visits
                   WHERE consent_analytics_allowed=true
                   GROUP BY (first_seen_at AT TIME ZONE 'UTC')::date,landing_path,locale
                   UNION ALL
                   SELECT (event.occurred_at AT TIME ZONE 'UTC')::date AS bucket_date,
                          visit.landing_path,visit.locale,0::bigint AS visits,
                          COUNT(*) FILTER (WHERE event.event_name='pageView')::bigint
                              AS page_views,
                          COUNT(*) FILTER (WHERE event.event_name='rfqStarted')::bigint
                              AS rfq_starts,
                          COUNT(*) FILTER (WHERE event.event_name='rfqSubmitted')::bigint
                              AS rfq_submissions
                   FROM analytics_events AS event
                   INNER JOIN guest_visits AS visit
                       ON visit.id=event.guest_visit_id
                      AND visit.anonymous_session_id=event.anonymous_session_id
                      AND visit.consent_record_id=event.consent_record_id
                   WHERE visit.consent_analytics_allowed=true
                     AND event.event_name IN ('pageView','rfqStarted','rfqSubmitted')
                   GROUP BY (event.occurred_at AT TIME ZONE 'UTC')::date,
                            visit.landing_path,visit.locale
               ), aggregated AS (
                   SELECT bucket_date,landing_path,locale,SUM(visits)::bigint AS visits,
                          SUM(page_views)::bigint AS page_views,
                          SUM(rfq_starts)::bigint AS rfq_starts,
                          SUM(rfq_submissions)::bigint AS rfq_submissions
                   FROM metric_rows
                   WHERE ($1::timestamptz IS NULL OR bucket_date >= ($1 AT TIME ZONE 'UTC')::date)
                     AND ($2::timestamptz IS NULL OR bucket_date < ($2 AT TIME ZONE 'UTC')::date)
                   GROUP BY bucket_date,landing_path,locale
               ), hashed AS (
                   SELECT aggregated.*,
                          guest_source_dimension_hash('','','','','',landing_path,locale)
                              AS dimension_hash
                   FROM aggregated
               ), guarded AS (
                   SELECT hashed.*,
                          COUNT(*) OVER (PARTITION BY bucket_date,dimension_hash)::bigint
                              AS hash_count
                   FROM hashed
               )
               SELECT bucket_date,landing_path,locale,visits,page_views,rfq_starts,
                      rfq_submissions,dimension_hash,hash_count
               FROM guarded
               WHERE ($3::date IS NULL OR bucket_date < $3
                      OR (bucket_date=$3 AND dimension_hash > $4::bytea))
               ORDER BY bucket_date DESC,dimension_hash
               LIMIT $5"#,
        )
        .bind(query.from)
        .bind(query.to)
        .bind(after.as_ref().map(|cursor| cursor.bucket_date))
        .bind(
            after
                .as_ref()
                .map(|cursor| cursor.dimension_hash.as_slice()),
        )
        .bind((limit + 1) as i64)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                ensure_no_analytics_hash_collision(row.try_get("hash_count")?)?;
                let cursor = analytics_cursor_from_database_row(&row)?;
                Ok::<_, ApiError>((
                    GuestVisitAggregate {
                        bucket_date: row.try_get("bucket_date")?,
                        landing_path: row.try_get("landing_path")?,
                        locale: row.try_get("locale")?,
                        visits: row.try_get("visits")?,
                        page_views: row.try_get("page_views")?,
                        rfq_starts: row.try_get("rfq_starts")?,
                        rfq_submissions: row.try_get("rfq_submissions")?,
                    },
                    cursor,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut aggregates = BTreeMap::<(chrono::NaiveDate, String, String), i64>::new();
        for visit in state.data.read().await.guest_visits.values() {
            let bucket_date = visit.first_seen_at.date_naive();
            if query
                .from
                .is_some_and(|from| bucket_date < from.date_naive())
                || query.to.is_some_and(|to| bucket_date >= to.date_naive())
            {
                continue;
            }
            let locale = visit
                .landing_path
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap_or("en")
                .to_owned();
            *aggregates
                .entry((bucket_date, visit.landing_path.clone(), locale))
                .or_default() += 1;
        }
        aggregates
            .into_iter()
            .map(|((bucket_date, landing_path, locale), visits)| {
                let cursor = AnalyticsCursor {
                    bucket_date,
                    dimension_hash: analytics_dimension_hash(&[
                        "",
                        "",
                        "",
                        "",
                        "",
                        &landing_path,
                        &locale,
                    ]),
                };
                (
                    GuestVisitAggregate {
                        bucket_date,
                        landing_path,
                        locale,
                        visits,
                        page_views: 0,
                        rfq_starts: 0,
                        rfq_submissions: 0,
                    },
                    cursor,
                )
            })
            .collect()
    };
    let page = if state.pool.is_some() {
        finish_analytics_page(&scope, values, limit)?
    } else {
        paginate_memory_analytics(&scope, values, after.as_ref(), limit)?
    };
    Ok(Json(page))
}

async fn list_guest_sources(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<CursorPage<GuestSourceDaily>>, ApiError> {
    validate_time_range(query.from, query.to)?;
    let scope = analytics_cursor_scope("admin.analytics.sources", query.from, query.to);
    let limit = cursor_limit(&query.pagination())?;
    let after = decode_analytics_cursor(&scope, query.cursor.as_deref())?;
    let values = if let Some(pool) = &state.pool {
        // Keep the acquisition dimensions byte-for-byte aligned with the
        // Worker aggregate so historical and live contributions collapse into
        // one daily source row.
        let rows = sqlx::query(
            r#"WITH metric_rows AS (
                   SELECT bucket_date,source_type,source_name,utm_source,utm_medium,
                          utm_campaign,landing_path,locale,visits,page_views,
                          rfq_starts,rfq_submissions
                   FROM guest_source_daily
                   UNION ALL
                   SELECT (first_seen_at AT TIME ZONE 'UTC')::date AS bucket_date,
                          source_type,COALESCE(referrer_host,'') AS source_name,
                          COALESCE(utm_source,'') AS utm_source,
                          COALESCE(utm_medium,'') AS utm_medium,
                          COALESCE(utm_campaign,'') AS utm_campaign,landing_path,locale,
                          COUNT(*)::bigint AS visits,0::bigint AS page_views,
                          0::bigint AS rfq_starts,0::bigint AS rfq_submissions
                   FROM guest_visits
                   WHERE consent_analytics_allowed=true
                   GROUP BY (first_seen_at AT TIME ZONE 'UTC')::date,source_type,
                            COALESCE(referrer_host,''),COALESCE(utm_source,''),
                            COALESCE(utm_medium,''),COALESCE(utm_campaign,''),
                            landing_path,locale
                   UNION ALL
                   SELECT (event.occurred_at AT TIME ZONE 'UTC')::date AS bucket_date,
                          visit.source_type,COALESCE(visit.referrer_host,'') AS source_name,
                          COALESCE(visit.utm_source,'') AS utm_source,
                          COALESCE(visit.utm_medium,'') AS utm_medium,
                          COALESCE(visit.utm_campaign,'') AS utm_campaign,
                          visit.landing_path,visit.locale,0::bigint AS visits,
                          COUNT(*) FILTER (WHERE event.event_name='pageView')::bigint
                              AS page_views,
                          COUNT(*) FILTER (WHERE event.event_name='rfqStarted')::bigint
                              AS rfq_starts,
                          COUNT(*) FILTER (WHERE event.event_name='rfqSubmitted')::bigint
                              AS rfq_submissions
                   FROM analytics_events AS event
                   INNER JOIN guest_visits AS visit
                       ON visit.id=event.guest_visit_id
                      AND visit.anonymous_session_id=event.anonymous_session_id
                      AND visit.consent_record_id=event.consent_record_id
                   WHERE visit.consent_analytics_allowed=true
                     AND event.event_name IN ('pageView','rfqStarted','rfqSubmitted')
                   GROUP BY (event.occurred_at AT TIME ZONE 'UTC')::date,
                            visit.source_type,COALESCE(visit.referrer_host,''),
                            COALESCE(visit.utm_source,''),COALESCE(visit.utm_medium,''),
                            COALESCE(visit.utm_campaign,''),visit.landing_path,visit.locale
               ), aggregated AS (
                   SELECT bucket_date,source_type,source_name,utm_source,utm_medium,
                          utm_campaign,landing_path,locale,SUM(visits)::bigint AS visits,
                          SUM(page_views)::bigint AS page_views,
                          SUM(rfq_starts)::bigint AS rfq_starts,
                          SUM(rfq_submissions)::bigint AS rfq_submissions
                   FROM metric_rows
                   WHERE ($1::timestamptz IS NULL OR bucket_date >= ($1 AT TIME ZONE 'UTC')::date)
                     AND ($2::timestamptz IS NULL OR bucket_date < ($2 AT TIME ZONE 'UTC')::date)
                   GROUP BY bucket_date,source_type,source_name,utm_source,utm_medium,
                            utm_campaign,landing_path,locale
               ), hashed AS (
                   SELECT aggregated.*,
                          guest_source_dimension_hash(
                              source_type,source_name,utm_source,utm_medium,utm_campaign,
                              landing_path,locale
                          ) AS dimension_hash
                   FROM aggregated
               ), guarded AS (
                   SELECT hashed.*,
                          COUNT(*) OVER (PARTITION BY bucket_date,dimension_hash)::bigint
                              AS hash_count
                   FROM hashed
               )
               SELECT bucket_date,source_type,
                      COALESCE(NULLIF(utm_source,''),NULLIF(source_name,''),source_type) AS source_name,
                      NULLIF(source_name,'') AS referrer_domain,
                      NULLIF(utm_source,'') AS utm_source,NULLIF(utm_medium,'') AS utm_medium,
                      NULLIF(utm_campaign,'') AS utm_campaign,landing_path,locale,
                      visits,page_views,rfq_starts,rfq_submissions,dimension_hash,hash_count
               FROM guarded
               WHERE ($3::date IS NULL OR bucket_date < $3
                      OR (bucket_date=$3 AND dimension_hash > $4::bytea))
               ORDER BY bucket_date DESC,dimension_hash
               LIMIT $5"#,
        )
        .bind(query.from)
        .bind(query.to)
        .bind(after.as_ref().map(|cursor| cursor.bucket_date))
        .bind(
            after
                .as_ref()
                .map(|cursor| cursor.dimension_hash.as_slice()),
        )
        .bind((limit + 1) as i64)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                ensure_no_analytics_hash_collision(row.try_get("hash_count")?)?;
                let cursor = analytics_cursor_from_database_row(&row)?;
                Ok((
                    GuestSourceDaily {
                        bucket_date: row.try_get("bucket_date")?,
                        source: row.try_get("source_type")?,
                        source_name: row.try_get("source_name")?,
                        referrer_domain: row.try_get("referrer_domain")?,
                        utm_source: row.try_get("utm_source")?,
                        medium: row.try_get("utm_medium")?,
                        campaign: row.try_get("utm_campaign")?,
                        landing_path: row.try_get("landing_path")?,
                        locale: row.try_get("locale")?,
                        visits: row.try_get("visits")?,
                        page_views: row.try_get("page_views")?,
                        rfq_starts: row.try_get("rfq_starts")?,
                        rfq_submissions: row.try_get("rfq_submissions")?,
                    },
                    cursor,
                ))
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        aggregate_memory_sources(&state, query.from, query.to)
            .await
            .into_iter()
            .map(|source| {
                let cursor = AnalyticsCursor {
                    bucket_date: source.bucket_date,
                    dimension_hash: analytics_dimension_hash(&[
                        &source.source,
                        source.referrer_domain.as_deref().unwrap_or(""),
                        source.utm_source.as_deref().unwrap_or(""),
                        source.medium.as_deref().unwrap_or(""),
                        source.campaign.as_deref().unwrap_or(""),
                        &source.landing_path,
                        &source.locale,
                    ]),
                };
                (source, cursor)
            })
            .collect()
    };
    let page = if state.pool.is_some() {
        finish_analytics_page(&scope, values, limit)?
    } else {
        paginate_memory_analytics(&scope, values, after.as_ref(), limit)?
    };
    Ok(Json(page))
}

async fn list_users(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<AdminUserRecord>>, ApiError> {
    let mut users = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT user_account.id,user_account.email,user_account.display_name,
                      user_account.locale,user_account.status,user_account.revision,
                      user_account.revision,
                      user_account.totp_confirmed_at IS NOT NULL AS totp_enabled,
                      user_account.invited_at,user_account.last_login_at,
                      user_account.created_at,user_account.updated_at,
                      COALESCE(array_agg(role.key ORDER BY role.key)
                        FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS roles
               FROM users user_account
               LEFT JOIN user_roles assignment ON assignment.user_id=user_account.id
               LEFT JOIN roles role ON role.id=assignment.role_id
               GROUP BY user_account.id ORDER BY user_account.created_at DESC"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(decode_admin_user)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let now = Utc::now();
        state
            .data
            .read()
            .await
            .admin_users
            .values()
            .map(|user| AdminUserRecord {
                id: user.id,
                email: user.email.clone(),
                display_name: user.display_name.clone(),
                locale: "zh-CN".into(),
                status: if user.active { "active" } else { "disabled" }.into(),
                revision: 1,
                roles: vec![user.role.clone()],
                totp_enabled: user.totp_enabled,
                invited_at: None,
                last_login_at: None,
                created_at: now,
                updated_at: now,
            })
            .collect()
    };
    users.sort_by_key(|user| std::cmp::Reverse(user.created_at));
    Ok(Json(paginate_by_id("admin.users", users, query, |user| {
        user.id
    })?))
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let user = load_admin_user(&state, id).await?;
    Ok(entity_response(StatusCode::OK, &user, user.revision))
}

async fn update_user(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(update): Json<UpdateAdminUser>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    if id == principal.user_id
        && update
            .status
            .as_deref()
            .is_some_and(|status| status != "active")
    {
        return Err(ApiError::conflict(
            "The current administrator cannot make their own account non-active.",
        ));
    }
    if update
        .status
        .as_deref()
        .is_some_and(|status| !matches!(status, "invited" | "active" | "disabled"))
    {
        return Err(ApiError::bad_request("status is invalid."));
    }
    let idempotency_request = json!({"userId": id, "update": &update});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.user.update",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let user: AdminUserRecord = replay.decode()?;
            return Ok(entity_response(status, &user, user.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_admin_user(&state, id).await?;
    if before.revision != expected {
        return Err(ApiError::conflict(
            "The administrator changed; reload before saving.",
        ));
    }
    let after = if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *transaction)
            .await?;
        // Serialize every identity update that can change the active Super
        // Admin set. Locking only the current user rows permits two concurrent
        // removals to both observe another administrator.
        sqlx::query(
            "SELECT pg_advisory_xact_lock(hashtextextended('airtek.identity.super-admin',0))",
        )
        .execute(&mut *transaction)
        .await?;
        let target = sqlx::query(
            r#"SELECT account.status,
                      EXISTS(
                        SELECT 1 FROM user_roles assignment
                        JOIN roles role ON role.id=assignment.role_id
                        WHERE assignment.user_id=account.id AND role.key='super-admin'
                      ) AS is_super_admin
               FROM users account WHERE account.id=$1 FOR UPDATE"#,
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| ApiError::not_found("Administrator was not found."))?;
        let current_status: String = target.try_get("status")?;
        let is_super_admin: bool = target.try_get("is_super_admin")?;
        let target_status_is_active =
            update.status.as_deref().unwrap_or(&current_status) == "active";
        let target_keeps_super_admin = update
            .role_keys
            .as_ref()
            .map(|roles| roles.iter().any(|role| role == "super-admin"))
            .unwrap_or(is_super_admin);
        let removes_super_admin = current_status == "active"
            && is_super_admin
            && (!target_status_is_active || !target_keeps_super_admin);
        if removes_super_admin {
            let active_super_admin_count = sqlx::query_scalar::<_, i64>(
                r#"SELECT count(DISTINCT account.id) FROM users account
                   JOIN user_roles assignment ON assignment.user_id=account.id
                   JOIN roles role ON role.id=assignment.role_id
                   WHERE account.status='active' AND role.key='super-admin'"#,
            )
            .fetch_one(&mut *transaction)
            .await?;
            if active_super_admin_count <= 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The last active Super Admin cannot become non-active or lose that role.",
                ));
            }
        }
        let result = sqlx::query(
            r#"UPDATE users SET display_name=COALESCE($2,display_name),
                      locale=COALESCE($3,locale),status=COALESCE($4,status),updated_at=$5,
                      revision=revision+1
               WHERE id=$1 AND revision=$6"#,
        )
        .bind(id)
        .bind(update.display_name.as_deref())
        .bind(update.locale.as_deref())
        .bind(update.status.as_deref())
        .bind(Utc::now())
        .bind(expected)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "The administrator changed; reload before saving.",
            ));
        }
        if let Some(role_keys) = &update.role_keys {
            validate_roles(&mut transaction, role_keys).await?;
            sqlx::query("DELETE FROM user_roles WHERE user_id=$1")
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                "INSERT INTO user_roles(user_id,role_id) SELECT $1,id FROM roles WHERE key=ANY($2)",
            )
            .bind(id)
            .bind(role_keys)
            .execute(&mut *transaction)
            .await?;
        }
        if let Some(status) = &update.status {
            if status != &before.status {
                sqlx::query(
                    r#"INSERT INTO user_status_history
                       (id,user_id,from_status,to_status,reason,changed_by,request_id,changed_at)
                       VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
                )
                .bind(Uuid::new_v4())
                .bind(id)
                .bind(&before.status)
                .bind(status)
                .bind(&update.reason)
                .bind(principal.user_id)
                .bind(request_id(&headers))
                .bind(Utc::now())
                .execute(&mut *transaction)
                .await?;
            }
        }
        let after = load_admin_user_in_transaction(&mut transaction, id).await?;
        let audit = mutation_audit_event(
            &headers,
            "identity.user.update",
            "user",
            Some(id),
            Some(json!(before)),
            Some(json!(after)),
            Some(update.reason),
        );
        insert_audit_event_in_transaction(&mut transaction, &audit).await?;
        let staged = idempotency
            .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
            .await?;
        transaction.commit().await?;
        staged.finish().await?;
        after
    } else {
        let removes_super_admin = before.status == "active"
            && before.roles.iter().any(|role| role == "super-admin")
            && (update
                .status
                .as_deref()
                .is_some_and(|status| status != "active")
                || update
                    .role_keys
                    .as_ref()
                    .is_some_and(|roles| !roles.iter().any(|role| role == "super-admin")));
        let mut data = state.data.write().await;
        if removes_super_admin
            && data
                .admin_users
                .values()
                .filter(|user| user.active && user.role == "super-admin")
                .count()
                <= 1
        {
            return Err(ApiError::conflict(
                "The last active Super Admin cannot become non-active or lose that role.",
            ));
        }
        if let Some(user) = data.admin_users.get_mut(&id) {
            if let Some(display_name) = &update.display_name {
                user.display_name = display_name.clone();
            }
            if let Some(status) = &update.status {
                user.active = status == "active";
            }
            if let Some(roles) = &update.role_keys {
                user.role = roles.first().cloned().unwrap_or_else(|| "auditor".into());
            }
        }
        drop(data);
        let after = load_admin_user(&state, id).await?;
        audit_mutation(
            &state,
            &headers,
            "identity.user.update",
            "user",
            Some(id),
            Some(json!(before)),
            Some(json!(after)),
            Some(update.reason),
        )
        .await?;
        idempotency.complete(&state, &after, StatusCode::OK).await?;
        after
    };
    Ok(entity_response(StatusCode::OK, &after, after.revision))
}

async fn revoke_user_sessions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if let Some(pool) = &state.pool {
        let mut transaction = pool.begin().await?;
        let sessions_revoked = sqlx::query(
            "UPDATE sessions SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=$1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let audit = mutation_audit_event(
            &headers,
            "identity.sessions.revoke",
            "user",
            Some(id),
            None,
            Some(json!({"sessionsRevoked": sessions_revoked})),
            Some("Revoke all sessions for administrator".into()),
        );
        insert_audit_event_in_transaction(&mut transaction, &audit).await?;
        transaction.commit().await?;
    } else {
        for session in state.data.write().await.admin_sessions.values_mut() {
            if session.user_id == id {
                session.revoked = true;
            }
        }
        audit_mutation(
            &state,
            &headers,
            "identity.sessions.revoke",
            "user",
            Some(id),
            None,
            Some(json!({"sessionsRevoked": true})),
            Some("Revoke all sessions for administrator".into()),
        )
        .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_roles(
    State(state): State<AppState>,
) -> Result<Json<CursorPage<AdminRoleRecord>>, ApiError> {
    let roles = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT role.id,role.key,role.display_name,role.system_role,role.revision,
                      COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                        FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
               FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
               GROUP BY role.id ORDER BY role.display_name"#,
        )
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AdminRoleRecord {
                    id: row.try_get("id")?,
                    key: row.try_get("key")?,
                    display_name: row.try_get("display_name")?,
                    system_role: row.try_get("system_role")?,
                    revision: row.try_get("revision")?,
                    permissions: row.try_get("permissions")?,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        let data = state.data.read().await;
        let mut seen = HashSet::new();
        data.admin_users
            .values()
            .filter(|user| seen.insert(user.role.clone()))
            .map(|user| AdminRoleRecord {
                id: Uuid::new_v4(),
                key: user.role.clone(),
                display_name: user.role.clone(),
                system_role: true,
                revision: 1,
                permissions: user.permissions.clone(),
            })
            .collect()
    };
    Ok(Json(CursorPage::all(roles)))
}

async fn get_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let role = load_admin_role(&state, id).await?;
    Ok(entity_response(StatusCode::OK, &role, role.revision))
}

async fn update_role(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(update): Json<UpdateAdminRole>,
) -> Result<Response, ApiError> {
    validate_reason(&update.reason)?;
    let expected = parse_if_match(&headers)?;
    if let Some(display_name) = update.display_name.as_deref() {
        if display_name.trim().is_empty() || display_name.len() > 120 {
            return Err(ApiError::bad_request(
                "displayName must contain 1 to 120 characters.",
            ));
        }
    }
    let idempotency_request = json!({"roleId": id, "update": &update});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.role.update",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let role: AdminRoleRecord = replay.decode()?;
            return Ok(entity_response(status, &role, role.revision));
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let before = load_admin_role(&state, id).await?;
    if before.revision != expected {
        return Err(ApiError::conflict(
            "The role changed; reload before saving.",
        ));
    }
    if before.key == "super-admin" {
        return Err(ApiError::forbidden(
            "The Super Admin role template is protected from permission edits.",
        ));
    }
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Role editing requires PostgreSQL persistence.",
        ));
    };
    let mut transaction = pool.begin().await?;
    if let Some(permissions) = &update.permissions {
        if permissions.len() > 128
            || permissions.iter().collect::<HashSet<_>>().len() != permissions.len()
        {
            return Err(ApiError::bad_request(
                "permissions must contain no more than 128 unique keys.",
            ));
        }
        let known =
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM permissions WHERE key=ANY($1)")
                .bind(permissions)
                .fetch_one(&mut *transaction)
                .await?;
        if known != permissions.len() as i64 {
            return Err(ApiError::bad_request(
                "permissions contains an unknown permission key.",
            ));
        }
    }
    let result = sqlx::query(
        r#"UPDATE roles SET display_name=COALESCE($2,display_name),revision=revision+1
           WHERE id=$1 AND revision=$3 AND key<>'super-admin'"#,
    )
    .bind(id)
    .bind(update.display_name.as_deref().map(str::trim))
    .bind(expected)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "The role changed; reload before saving.",
        ));
    }
    if let Some(permissions) = &update.permissions {
        sqlx::query("DELETE FROM role_permissions WHERE role_id=$1")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "INSERT INTO role_permissions(role_id,permission_key) SELECT $1,key FROM permissions WHERE key=ANY($2)",
        )
        .bind(id)
        .bind(permissions)
        .execute(&mut *transaction)
        .await?;
    }
    let after = load_admin_role_in_transaction(&mut transaction, id).await?;
    let audit = mutation_audit_event(
        &headers,
        "identity.role.update",
        "role",
        Some(id),
        Some(json!(before)),
        Some(json!(after)),
        Some(update.reason),
    );
    insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &after, StatusCode::OK)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(entity_response(StatusCode::OK, &after, after.revision))
}

async fn list_invitations(
    State(state): State<AppState>,
) -> Result<Json<CursorPage<UserInvitation>>, ApiError> {
    let Some(pool) = &state.pool else {
        return Ok(Json(CursorPage::all(Vec::new())));
    };
    let rows = sqlx::query(
        r#"SELECT invitation.id,invitation.email,invitation.display_name,
                  invitation.locale,invitation.invited_at,invitation.expires_at,
                  invitation.accepted_at,invitation.revoked_at,
                  COALESCE(array_agg(role.key ORDER BY role.key)
                    FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS role_keys
           FROM user_invitations invitation
           LEFT JOIN user_invitation_roles assignment ON assignment.invitation_id=invitation.id
           LEFT JOIN roles role ON role.id=assignment.role_id
           GROUP BY invitation.id ORDER BY invitation.invited_at DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(Json(CursorPage::all(
        rows.into_iter()
            .map(decode_invitation)
            .collect::<Result<Vec<_>, _>>()?,
    )))
}

async fn invite_user(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(input): Json<InviteAdminUser>,
) -> Result<Response, ApiError> {
    validate_invitation(&input)?;
    let replay_key = state
        .config
        .invitation_replay_encryption_key
        .as_ref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Invitation replay encryption is not configured for this deployment.",
            )
        })?;
    let replay_aad = invitation_replay_aad(&headers, &input)?;
    let idempotency =
        match begin_idempotency(&state, "admin.identity.invitation.create", &headers, &input)
            .await?
        {
            IdempotencyOutcome::Replay(replay) => {
                let status = replay.status()?;
                let encrypted: EncryptedInvitationReplay = replay.decode()?;
                let invitation =
                    open_invitation_replay(&encrypted, replay_key, replay_aad.as_bytes())?;
                let mut response = (status, Json(invitation)).into_response();
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-store, max-age=0"),
                );
                return Ok(response);
            }
            IdempotencyOutcome::Fresh(context) => context,
        };
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Invitations require PostgreSQL persistence.",
        ));
    };
    let mut token_bytes = [0_u8; 32];
    SystemRandom::new()
        .fill(&mut token_bytes)
        .map_err(|_| ApiError::internal("Secure invitation token generation failed."))?;
    let token = URL_SAFE_NO_PAD.encode(token_bytes);
    let now = Utc::now();
    let id = Uuid::new_v4();
    let expires_at = now + Duration::days(7);
    let normalized_email = input.email.trim().to_lowercase();
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "airtek.identity.invitation.email:{normalized_email}"
        ))
        .execute(&mut *transaction)
        .await?;
    let expired_invitation_id = sqlx::query_scalar::<_, Uuid>(
        r#"UPDATE user_invitations SET status='expired'
           WHERE lower(email)=lower($1) AND status='pending' AND expires_at<=now()
             AND accepted_at IS NULL AND revoked_at IS NULL
           RETURNING id"#,
    )
    .bind(&normalized_email)
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(expired_id) = expired_invitation_id {
        let audit = mutation_audit_event(
            &headers,
            "identity.invitation.expire",
            "userInvitation",
            Some(expired_id),
            Some(json!({"status": "pending", "email": normalized_email})),
            Some(json!({"status": "expired", "email": normalized_email})),
            Some("Expire superseded administrator invitation".into()),
        );
        insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    }
    let account_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($1))")
            .bind(&normalized_email)
            .fetch_one(&mut *transaction)
            .await?;
    if account_exists {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "An administrator account already exists for this email.",
        ));
    }
    let pending_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM user_invitations WHERE lower(email)=lower($1) AND status='pending')",
    )
    .bind(&normalized_email)
    .fetch_one(&mut *transaction)
    .await?;
    if pending_exists {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "A current administrator invitation already exists for this email.",
        ));
    }
    validate_roles(&mut transaction, &input.role_keys).await?;
    sqlx::query(
        r#"INSERT INTO user_invitations
           (id,email,display_name,locale,token_hash,invited_by,invited_at,expires_at)
           VALUES ($1,$2,$3,'zh-CN',$4,$5,$6,$7)"#,
    )
    .bind(id)
    .bind(&normalized_email)
    .bind(input.display_name.trim())
    .bind(Sha256::digest(token.as_bytes()).to_vec())
    .bind(principal.user_id)
    .bind(now)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO user_invitation_roles(invitation_id,role_id) SELECT $1,id FROM roles WHERE key=ANY($2)",
    )
    .bind(id)
    .bind(&input.role_keys)
    .execute(&mut *transaction)
    .await?;
    let invitation = UserInvitation {
        id,
        email: normalized_email,
        display_name: input.display_name.trim().to_owned(),
        locale: "zh-CN".into(),
        role_keys: input.role_keys,
        status: "pending".into(),
        invited_at: now,
        expires_at,
        invitation_token: Some(token),
    };
    let audit = mutation_audit_event(
        &headers,
        "identity.invitation.create",
        "userInvitation",
        Some(id),
        None,
        Some(json!({
            "id": invitation.id,
            "email": invitation.email,
            "roles": invitation.role_keys,
            "expiresAt": invitation.expires_at,
        })),
        Some("Invite administrator".into()),
    );
    insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    let encrypted_replay = seal_invitation_replay(&invitation, replay_key, replay_aad.as_bytes())?;
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &encrypted_replay, StatusCode::CREATED)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    let mut response = (StatusCode::CREATED, Json(invitation)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}

async fn revoke_invitation(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<ReasonRequest>,
) -> Result<Response, ApiError> {
    validate_reason(&request.reason)?;
    let idempotency_request = json!({"invitationId": id, "request": &request});
    let idempotency = match begin_idempotency(
        &state,
        "admin.identity.invitation.revoke",
        &headers,
        &idempotency_request,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let _: Value = replay.decode()?;
            return Ok(status.into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Invitations require PostgreSQL persistence.",
        ));
    };
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        r#"UPDATE user_invitations SET status='revoked',revoked_at=now(),revoked_by=$2,revoke_reason=$3
           WHERE id=$1 AND status='pending' AND expires_at>now()
             AND accepted_at IS NULL AND revoked_at IS NULL"#,
    )
    .bind(id)
    .bind(principal.user_id)
    .bind(&request.reason)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "Invitation is missing, accepted, or already revoked.",
        ));
    }
    let audit = mutation_audit_event(
        &headers,
        "identity.invitation.revoke",
        "userInvitation",
        Some(id),
        None,
        Some(json!({"revoked": true})),
        Some(request.reason),
    );
    insert_audit_event_in_transaction(&mut transaction, &audit).await?;
    let replay_receipt = json!({"invitationId": id, "revoked": true});
    let staged = idempotency
        .stage_in_transaction(&mut transaction, &replay_receipt, StatusCode::NO_CONTENT)
        .await?;
    transaction.commit().await?;
    staged.finish().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---- Persistence helpers ------------------------------------------------------------------

async fn load_admin_product_detail(
    state: &AppState,
    id: Uuid,
) -> Result<AdminProductDetail, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT payload,data_origin,current_revision,product_import_run_id,stable_id
               FROM products WHERE id=$1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Product was not found."))?;
        let mut product: crate::models::Product = decode_json(row.try_get("payload")?, "product")?;
        let origin: String = row.try_get("data_origin")?;
        let presentation_row = sqlx::query(
            r#"SELECT locale,slug,title,summary,content,seo_metadata,indexable,
                      current_revision,published_revision,updated_at
               FROM product_presentation_working
               WHERE product_id=$1
               ORDER BY CASE WHEN locale=$2 THEN 0 ELSE 1 END,locale LIMIT 1"#,
        )
        .bind(id)
        .bind(&product.locale)
        .fetch_optional(pool)
        .await?;
        let presentation = presentation_row
            .map(|presentation| {
                let content: Value = presentation.try_get("content")?;
                let sort_order = content
                    .get("sortOrder")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or_default();
                let related_content_ids = content
                    .get("relatedContentIds")
                    .cloned()
                    .map(|value| decode_json(value, "product related content ids"))
                    .transpose()?
                    .unwrap_or_default();
                Ok::<ProductPresentation, ApiError>(ProductPresentation {
                    locale: presentation.try_get("locale")?,
                    slug: presentation.try_get("slug")?,
                    title: presentation.try_get("title")?,
                    summary: presentation.try_get("summary")?,
                    seo: decode_json(
                        presentation.try_get("seo_metadata")?,
                        "product presentation SEO",
                    )?,
                    indexable: presentation.try_get("indexable")?,
                    sort_order,
                    related_content_ids,
                    revision: presentation.try_get("current_revision")?,
                    published_revision: presentation.try_get("published_revision")?,
                    updated_at: presentation.try_get("updated_at")?,
                })
            })
            .transpose()?;
        if let Some(presentation) = &presentation {
            overlay_product_presentation(&mut product, presentation);
        }
        let import_run_id: Option<Uuid> = row.try_get("product_import_run_id")?;
        let missing_assets = if let Some(import_run_id) = import_run_id {
            sqlx::query(
                r#"SELECT source_record_id,asset_type,source_reference
                   FROM product_import_missing_assets
                   WHERE import_run_id=$1 AND source_record_id=$2 AND resolution_status='missing'
                   ORDER BY created_at,id"#,
            )
            .bind(import_run_id)
            .bind(row.try_get::<String, _>("stable_id")?)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|missing| {
                Ok(crate::models::MissingAssetReference {
                    stable_id: missing.try_get("source_record_id")?,
                    asset_type: missing.try_get("asset_type")?,
                    source_reference: missing.try_get("source_reference")?,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
        } else {
            Vec::new()
        };
        return Ok(AdminProductDetail {
            product,
            source_kind: match origin.as_str() {
                "verifiedCsv" => DataClass::VerifiedCsv,
                "developmentFixture" => DataClass::DevelopmentFixture,
                "feishu" => DataClass::Feishu,
                _ => DataClass::Editorial,
            },
            missing_assets,
            presentation,
        });
    }
    let data = state.data.read().await;
    let mut product = data
        .products
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Product was not found."))?;
    let presentation = data
        .product_presentations
        .get(&(id, product.locale.clone()))
        .cloned()
        .unwrap_or_else(|| ProductPresentation {
            locale: product.locale.clone(),
            slug: product.slug.clone(),
            title: product.title.clone(),
            summary: product.summary.clone(),
            seo: product.seo.clone(),
            indexable: product.indexable,
            sort_order: product.sort_order,
            related_content_ids: product.related_content_ids.clone(),
            revision: 1,
            published_revision: product.published_revision.map(|_| 1),
            updated_at: product.updated_at,
        });
    overlay_product_presentation(&mut product, &presentation);
    Ok(AdminProductDetail {
        presentation: Some(presentation),
        product,
        source_kind: DataClass::Feishu,
        missing_assets: Vec::new(),
    })
}

fn overlay_product_presentation(
    product: &mut crate::models::Product,
    presentation: &ProductPresentation,
) {
    product.slug = presentation.slug.clone();
    product.locale = presentation.locale.clone();
    product.title = presentation.title.clone();
    product.summary = presentation.summary.clone();
    product.seo = presentation.seo.clone();
    product.indexable = presentation.indexable;
    product.sort_order = presentation.sort_order;
    product.related_content_ids = presentation.related_content_ids.clone();
}

async fn load_news(state: &AppState, id: Uuid) -> Result<NewsEntry, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT entry.payload,news.category,news.author_display_name,
                      news.cover_media_asset_id,news.publication_at,news.featured,
                      news.data_origin
               FROM content_entries entry JOIN news_working news
                 ON news.content_id=entry.id
               WHERE entry.id=$1 AND entry.kind='news'"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        return row
            .map(decode_news_row)
            .transpose()?
            .ok_or_else(|| ApiError::not_found("News entry was not found."));
    }
    state
        .data
        .read()
        .await
        .news
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("News entry was not found."))
}

async fn load_news_revision(
    state: &AppState,
    id: Uuid,
    revision: i64,
) -> Result<NewsEntry, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT content_revision.payload,news.category,news.author_display_name,
                      news.cover_media_asset_id,news.publication_at,news.featured,
                      news.data_origin
               FROM content_revisions content_revision
               JOIN content_entries entry ON entry.id=content_revision.content_id
               JOIN news ON news.content_id=content_revision.content_id
                 AND news.revision=content_revision.revision
               WHERE content_revision.content_id=$1 AND content_revision.revision=$2"#,
        )
        .bind(id)
        .bind(revision)
        .fetch_optional(pool)
        .await?;
        return row
            .map(decode_news_row)
            .transpose()?
            .ok_or_else(|| ApiError::not_found("News revision was not found."));
    }
    let data = state.data.read().await;
    let content = data
        .content_revisions
        .get(&id)
        .and_then(|values| values.get(&revision))
        .cloned()
        .ok_or_else(|| ApiError::not_found("News revision was not found."))?;
    let metadata = data
        .news
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("News metadata was not found."))?;
    Ok(NewsEntry {
        content,
        ..metadata
    })
}

fn decode_news_row(row: sqlx::postgres::PgRow) -> Result<NewsEntry, ApiError> {
    Ok(NewsEntry {
        content: decode_json(row.try_get("payload")?, "news content")?,
        category: row.try_get("category")?,
        author_display_name: row.try_get("author_display_name")?,
        cover_media_id: row.try_get("cover_media_asset_id")?,
        published_at: row.try_get("publication_at")?,
        featured: row.try_get("featured")?,
        data_class: match row.try_get::<String, _>("data_origin")?.as_str() {
            "developmentFixture" => DataClass::DevelopmentFixture,
            _ => DataClass::Editorial,
        },
    })
}

async fn persist_news_revision(
    state: &AppState,
    entry: &NewsEntry,
    expected_revision: Option<i64>,
    publish: bool,
    actor: &str,
    audit: &AuditEvent,
) -> Result<(), ApiError> {
    let Some(pool) = &state.pool else {
        return Ok(());
    };
    let payload = serde_json::to_value(&entry.content)
        .map_err(|_| ApiError::internal("News serialization failed."))?;
    let origin = if entry.data_class == DataClass::DevelopmentFixture {
        "developmentFixture"
    } else {
        "editorial"
    };
    let mut transaction = pool.begin().await?;
    match expected_revision {
        None => {
            sqlx::query(
                r#"INSERT INTO content_entries
                   (id,kind,slug,locale,title,status,is_placeholder,current_revision,
                    published_revision,scheduled_for,payload,updated_at,data_origin)
                   VALUES ($1,'news',$2,$3,$4,'draft',$5,1,NULL,NULL,$6,$7,$8)"#,
            )
            .bind(entry.content.id)
            .bind(&entry.content.slug)
            .bind(&entry.content.locale)
            .bind(&entry.content.title)
            .bind(entry.content.is_placeholder)
            .bind(&payload)
            .bind(entry.content.updated_at)
            .bind(origin)
            .execute(&mut *transaction)
            .await?;
        }
        Some(expected) => {
            let status = if publish { "published" } else { "draft" };
            let published_revision = publish.then_some(entry.content.current_revision);
            let result = sqlx::query(
                r#"UPDATE content_entries SET slug=$2,locale=$3,title=$4,status=$5,
                          is_placeholder=$6,current_revision=$7,published_revision=COALESCE($8,published_revision),
                          scheduled_for=NULL,payload=$9,updated_at=$10,data_origin=$11
                   WHERE id=$1 AND current_revision=$12 AND kind='news'"#,
            )
            .bind(entry.content.id)
            .bind(&entry.content.slug)
            .bind(&entry.content.locale)
            .bind(&entry.content.title)
            .bind(status)
            .bind(entry.content.is_placeholder)
            .bind(entry.content.current_revision)
            .bind(published_revision)
            .bind(&payload)
            .bind(entry.content.updated_at)
            .bind(origin)
            .bind(expected)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The news entry changed; reload before saving.",
                ));
            }
        }
    }
    sqlx::query(
        r#"INSERT INTO news_working
           (content_id,content_kind,category,author_display_name,
            cover_media_asset_id,featured,publication_at,reading_minutes,data_origin,updated_at)
           VALUES ($1,'news',$2,$3,$4,$5,$6,NULL,$7,$8)
           ON CONFLICT (content_id) DO UPDATE SET
             category=EXCLUDED.category,
             author_display_name=EXCLUDED.author_display_name,
             cover_media_asset_id=EXCLUDED.cover_media_asset_id,
             featured=EXCLUDED.featured,
             publication_at=EXCLUDED.publication_at,
             reading_minutes=EXCLUDED.reading_minutes,
             data_origin=EXCLUDED.data_origin,
             updated_at=EXCLUDED.updated_at"#,
    )
    .bind(entry.content.id)
    .bind(&entry.category)
    .bind(entry.author_display_name.as_deref())
    .bind(entry.cover_media_id)
    .bind(entry.featured)
    .bind(entry.published_at)
    .bind(origin)
    .bind(entry.content.updated_at)
    .execute(&mut *transaction)
    .await?;
    let publication_event = if expected_revision == Some(entry.content.current_revision) {
        "publish"
    } else {
        "rollback"
    };
    if publish {
        sqlx::query(
            r#"INSERT INTO content_revisions(content_id,revision,payload,created_by,created_at)
               VALUES ($1,$2,$3,$4,$5)
               ON CONFLICT (content_id,revision) DO NOTHING"#,
        )
        .bind(entry.content.id)
        .bind(entry.content.current_revision)
        .bind(&payload)
        .bind(actor)
        .bind(entry.content.updated_at)
        .execute(&mut *transaction)
        .await?;
    }
    if publish {
        sqlx::query(
            r#"INSERT INTO news
               (content_id,revision,content_kind,category,author_display_name,
                cover_media_asset_id,featured,publication_at,reading_minutes,data_origin)
               VALUES ($1,$2,'news',$3,$4,$5,$6,$7,NULL,$8)
               ON CONFLICT (content_id,revision) DO NOTHING"#,
        )
        .bind(entry.content.id)
        .bind(entry.content.current_revision)
        .bind(&entry.category)
        .bind(entry.author_display_name.as_deref())
        .bind(entry.cover_media_id)
        .bind(entry.featured)
        .bind(entry.published_at)
        .bind(origin)
        .execute(&mut *transaction)
        .await?;
        let canonical_path = entry
            .content
            .seo
            .canonical_path
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("Published News requires canonicalPath."))?;
        sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
            .bind(entry.content.id)
            .execute(&mut *transaction)
            .await?;
        let owner = sqlx::query_scalar::<_, Uuid>(
            "SELECT entity_id FROM public_routes WHERE canonical_path=$1",
        )
        .bind(canonical_path)
        .fetch_optional(&mut *transaction)
        .await?;
        if owner.is_some_and(|owner| owner != entry.content.id) {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "Another published entity already owns this News canonical path.",
            ));
        }
        sqlx::query(
            r#"INSERT INTO public_routes
               (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
               VALUES ($1,'content',$2,$3,$4,$5,$6)
               ON CONFLICT (entity_type,entity_id,locale) DO UPDATE SET
                 canonical_path=EXCLUDED.canonical_path,indexable=EXCLUDED.indexable,
                 updated_at=EXCLUDED.updated_at"#,
        )
        .bind(Uuid::new_v4())
        .bind(entry.content.id)
        .bind(&entry.content.locale)
        .bind(canonical_path)
        .bind(entry.content.seo.indexable && !entry.content.is_placeholder)
        .bind(entry.content.updated_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
               VALUES ($1,'public.news.published','content',$2,$3)"#,
        )
        .bind(Uuid::new_v4())
        .bind(entry.content.id)
        .bind(json!({
            "entityId": entry.content.id,
            "revision": entry.content.current_revision,
            "locale": entry.content.locale,
            "event": publication_event
        }))
        .execute(&mut *transaction)
        .await?;
    }
    insert_audit_event_in_transaction(&mut transaction, audit).await?;
    transaction.commit().await?;
    Ok(())
}

async fn load_general_information_by_locale(
    state: &AppState,
    locale: &str,
) -> Result<Option<GeneralInformation>, ApiError> {
    if let Some(pool) = &state.pool {
        return sqlx::query(
            r#"SELECT id,locale,payload,status,current_revision,published_revision,
                      is_placeholder,updated_at FROM general_information
               WHERE scope='site' AND locale=$1"#,
        )
        .bind(locale)
        .fetch_optional(pool)
        .await?
        .map(decode_general_information)
        .transpose();
    }
    Ok(state
        .data
        .read()
        .await
        .general_information
        .values()
        .find(|entry| entry.locale == locale)
        .cloned())
}

async fn load_general_information(
    state: &AppState,
    id: Uuid,
) -> Result<GeneralInformation, ApiError> {
    if let Some(pool) = &state.pool {
        return sqlx::query(
            r#"SELECT id,locale,payload,status,current_revision,published_revision,
                      is_placeholder,updated_at FROM general_information WHERE id=$1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(decode_general_information)
        .transpose()?
        .ok_or_else(|| ApiError::not_found("General Information was not found."));
    }
    state
        .data
        .read()
        .await
        .general_information
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("General Information was not found."))
}

async fn load_general_information_revision(
    state: &AppState,
    id: Uuid,
    revision_number: i64,
) -> Result<GeneralInformation, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT information.id,revision.locale,revision.payload,
                      revision.is_placeholder,revision.created_at
               FROM general_information information
               JOIN general_information_revisions revision
                 ON revision.general_information_id=information.id
               WHERE information.id=$1 AND revision.revision=$2"#,
        )
        .bind(id)
        .bind(revision_number)
        .fetch_optional(pool)
        .await?;
        return row
            .map(|row| {
                Ok::<GeneralInformation, ApiError>(GeneralInformation {
                    id: row.try_get("id")?,
                    locale: row.try_get("locale")?,
                    payload: row.try_get("payload")?,
                    status: PublicationStatus::Draft,
                    current_revision: revision_number,
                    published_revision: None,
                    is_placeholder: row.try_get("is_placeholder")?,
                    updated_at: row.try_get("created_at")?,
                })
            })
            .transpose()?
            .ok_or_else(|| ApiError::not_found("General Information revision was not found."));
    }
    state
        .data
        .read()
        .await
        .general_information_revisions
        .get(&id)
        .and_then(|values| values.get(&revision_number))
        .cloned()
        .ok_or_else(|| ApiError::not_found("General Information revision was not found."))
}

fn decode_general_information(row: sqlx::postgres::PgRow) -> Result<GeneralInformation, ApiError> {
    Ok(GeneralInformation {
        id: row.try_get("id")?,
        locale: row.try_get("locale")?,
        payload: row.try_get("payload")?,
        status: decode_publication_status(row.try_get("status")?),
        current_revision: row.try_get("current_revision")?,
        published_revision: row.try_get("published_revision")?,
        is_placeholder: row.try_get("is_placeholder")?,
        updated_at: row.try_get("updated_at")?,
    })
}

async fn persist_general_information_revision(
    state: &AppState,
    entry: &GeneralInformation,
    expected_revision: Option<i64>,
    publish: bool,
    actor: &str,
    audit: &AuditEvent,
    publication_event: Option<&str>,
) -> Result<(), ApiError> {
    let Some(pool) = &state.pool else {
        return Ok(());
    };
    let mut transaction = pool.begin().await?;
    let origin = match expected_revision {
        None => {
            sqlx::query(
                r#"INSERT INTO general_information
                   (id,scope,locale,status,is_placeholder,data_origin,current_revision,
                    published_revision,payload,updated_by,updated_at)
                   VALUES ($1,'site',$2,'draft',$3,'editorial',1,NULL,$4,$5,$6)"#,
            )
            .bind(entry.id)
            .bind(&entry.locale)
            .bind(entry.is_placeholder)
            .bind(&entry.payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            "editorial".to_owned()
        }
        Some(expected) => {
            let result = sqlx::query(
                r#"UPDATE general_information SET locale=$2,status=$3,is_placeholder=$4,
                          data_origin=CASE
                            WHEN data_origin='developmentFixture' AND NOT $4 THEN 'editorial'
                            ELSE data_origin
                          END,
                          current_revision=$5,published_revision=COALESCE($6,published_revision),
                          scheduled_for=NULL,payload=$7,updated_by=$8,updated_at=$9
                   WHERE id=$1 AND current_revision=$10
                   RETURNING data_origin"#,
            )
            .bind(entry.id)
            .bind(&entry.locale)
            .bind(if publish { "published" } else { "draft" })
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(publish.then_some(entry.current_revision))
            .bind(&entry.payload)
            .bind(actor)
            .bind(entry.updated_at)
            .bind(expected)
            .fetch_optional(&mut *transaction)
            .await?;
            let Some(result) = result else {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "General Information changed; reload before saving.",
                ));
            };
            result.try_get::<String, _>("data_origin")?
        }
    };
    let immutable_revision_matches = sqlx::query_scalar::<_, bool>(
        r#"WITH inserted AS (
               INSERT INTO general_information_revisions
                   (general_information_id,revision,payload,locale,is_placeholder,data_origin,
                    created_by,created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT (general_information_id,revision) DO NOTHING
               RETURNING true
           )
           SELECT EXISTS(SELECT 1 FROM inserted)
              OR EXISTS (
                   SELECT 1 FROM general_information_revisions
                   WHERE general_information_id=$1 AND revision=$2
                     AND payload=$3 AND locale=$4 AND is_placeholder=$5
                     AND data_origin=$6
              )"#,
    )
    .bind(entry.id)
    .bind(entry.current_revision)
    .bind(&entry.payload)
    .bind(&entry.locale)
    .bind(entry.is_placeholder)
    .bind(&origin)
    .bind(actor)
    .bind(entry.updated_at)
    .fetch_one(&mut *transaction)
    .await?;
    if !immutable_revision_matches {
        transaction.rollback().await?;
        return Err(ApiError::service_unavailable(
            "Stored General Information revision conflicts with immutable history.",
        ));
    }
    if publish {
        sqlx::query(
            r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
               VALUES ($1,'public.generalInformation.published','generalInformation',$2,$3)"#,
        )
        .bind(Uuid::new_v4())
        .bind(entry.id)
        .bind(json!({
            "entityId": entry.id,
            "revision": entry.current_revision,
            "locale": entry.locale,
            "event": publication_event.unwrap_or("publish")
        }))
        .execute(&mut *transaction)
        .await?;
    }
    insert_audit_event_in_transaction(&mut transaction, audit).await?;
    transaction.commit().await?;
    Ok(())
}

async fn load_admin_user(state: &AppState, id: Uuid) -> Result<AdminUserRecord, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT user_account.id,user_account.email,user_account.display_name,
                      user_account.locale,user_account.status,user_account.revision,
                      user_account.totp_confirmed_at IS NOT NULL AS totp_enabled,
                      user_account.invited_at,user_account.last_login_at,
                      user_account.created_at,user_account.updated_at,
                      COALESCE(array_agg(role.key ORDER BY role.key)
                        FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS roles
               FROM users user_account
               LEFT JOIN user_roles assignment ON assignment.user_id=user_account.id
               LEFT JOIN roles role ON role.id=assignment.role_id
               WHERE user_account.id=$1 GROUP BY user_account.id"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        return row
            .map(decode_admin_user)
            .transpose()?
            .ok_or_else(|| ApiError::not_found("Administrator was not found."));
    }
    let user = state
        .data
        .read()
        .await
        .admin_users
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("Administrator was not found."))?;
    let now = Utc::now();
    Ok(AdminUserRecord {
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        locale: "zh-CN".into(),
        status: if user.active { "active" } else { "disabled" }.into(),
        revision: 1,
        roles: vec![user.role],
        totp_enabled: user.totp_enabled,
        invited_at: None,
        last_login_at: None,
        created_at: now,
        updated_at: now,
    })
}

async fn load_admin_user_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<AdminUserRecord, ApiError> {
    let row = sqlx::query(
        r#"SELECT user_account.id,user_account.email,user_account.display_name,
                  user_account.locale,user_account.status,user_account.revision,
                  user_account.totp_confirmed_at IS NOT NULL AS totp_enabled,
                  user_account.invited_at,user_account.last_login_at,
                  user_account.created_at,user_account.updated_at,
                  COALESCE(array_agg(role.key ORDER BY role.key)
                    FILTER (WHERE role.key IS NOT NULL),ARRAY[]::text[]) AS roles
           FROM users user_account
           LEFT JOIN user_roles assignment ON assignment.user_id=user_account.id
           LEFT JOIN roles role ON role.id=assignment.role_id
           WHERE user_account.id=$1 GROUP BY user_account.id"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Administrator was not found."))?;
    decode_admin_user(row)
}

async fn load_admin_role(state: &AppState, id: Uuid) -> Result<AdminRoleRecord, ApiError> {
    let Some(pool) = &state.pool else {
        return Err(ApiError::service_unavailable(
            "Role details require PostgreSQL persistence.",
        ));
    };
    let row = sqlx::query(
        r#"SELECT role.id,role.key,role.display_name,role.system_role,role.revision,
                  COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                    FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
           FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
           WHERE role.id=$1 GROUP BY role.id"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Role was not found."))?;
    Ok(AdminRoleRecord {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        display_name: row.try_get("display_name")?,
        system_role: row.try_get("system_role")?,
        revision: row.try_get("revision")?,
        permissions: row.try_get("permissions")?,
    })
}

async fn load_admin_role_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<AdminRoleRecord, ApiError> {
    let row = sqlx::query(
        r#"SELECT role.id,role.key,role.display_name,role.system_role,role.revision,
                  COALESCE(array_agg(permission.permission_key ORDER BY permission.permission_key)
                    FILTER (WHERE permission.permission_key IS NOT NULL),ARRAY[]::text[]) AS permissions
           FROM roles role LEFT JOIN role_permissions permission ON permission.role_id=role.id
           WHERE role.id=$1 GROUP BY role.id"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Role was not found."))?;
    Ok(AdminRoleRecord {
        id: row.try_get("id")?,
        key: row.try_get("key")?,
        display_name: row.try_get("display_name")?,
        system_role: row.try_get("system_role")?,
        revision: row.try_get("revision")?,
        permissions: row.try_get("permissions")?,
    })
}

fn decode_admin_user(row: sqlx::postgres::PgRow) -> Result<AdminUserRecord, ApiError> {
    Ok(AdminUserRecord {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        locale: row.try_get("locale")?,
        status: row.try_get("status")?,
        revision: row.try_get("revision")?,
        roles: row.try_get("roles")?,
        totp_enabled: row.try_get("totp_enabled")?,
        invited_at: row.try_get("invited_at")?,
        last_login_at: row.try_get("last_login_at")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn decode_invitation(row: sqlx::postgres::PgRow) -> Result<UserInvitation, ApiError> {
    let accepted: Option<chrono::DateTime<Utc>> = row.try_get("accepted_at")?;
    let revoked: Option<chrono::DateTime<Utc>> = row.try_get("revoked_at")?;
    let expires_at = row.try_get("expires_at")?;
    let status = if accepted.is_some() {
        "accepted"
    } else if revoked.is_some() {
        "revoked"
    } else if expires_at <= Utc::now() {
        "expired"
    } else {
        "pending"
    };
    Ok(UserInvitation {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        locale: row.try_get("locale")?,
        role_keys: row.try_get("role_keys")?,
        status: status.into(),
        invited_at: row.try_get("invited_at")?,
        expires_at,
        invitation_token: None,
    })
}

async fn validate_roles(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    role_keys: &[String],
) -> Result<(), ApiError> {
    if role_keys.is_empty() || role_keys.len() > 20 {
        return Err(ApiError::bad_request(
            "roleKeys must contain 1 to 20 roles.",
        ));
    }
    let unique = role_keys.iter().collect::<HashSet<_>>().len() as i64;
    let count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM roles WHERE key=ANY($1)")
        .bind(role_keys)
        .fetch_one(&mut **transaction)
        .await?;
    if count != unique || unique != role_keys.len() as i64 {
        return Err(ApiError::bad_request(
            "roleKeys contains an unknown or duplicate role.",
        ));
    }
    Ok(())
}

async fn aggregate_memory_sources(
    state: &AppState,
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
) -> Vec<GuestSourceDaily> {
    let mut values = BTreeMap::new();
    for visit in state.data.read().await.guest_visits.values() {
        let bucket_date = visit.first_seen_at.date_naive();
        if from
            .map(|from| bucket_date < from.date_naive())
            .unwrap_or(false)
            || to.map(|to| bucket_date >= to.date_naive()).unwrap_or(false)
        {
            continue;
        }
        *values
            .entry((
                bucket_date,
                visit.source.clone(),
                visit.referrer_domain.clone(),
                visit.medium.clone(),
                visit.campaign.clone(),
                visit.landing_path.clone(),
                visit
                    .landing_path
                    .trim_start_matches('/')
                    .split('/')
                    .next()
                    .unwrap_or("en")
                    .to_owned(),
            ))
            .or_default() += 1;
    }
    values
        .into_iter()
        .map(
            |(
                (bucket_date, source, referrer_domain, medium, campaign, landing_path, locale),
                visits,
            )| GuestSourceDaily {
                bucket_date,
                source_name: Some(referrer_domain.clone().unwrap_or_else(|| source.clone())),
                referrer_domain,
                source,
                utm_source: None,
                medium,
                campaign,
                landing_path,
                locale,
                visits,
                page_views: 0,
                rfq_starts: 0,
                rfq_submissions: 0,
            },
        )
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn audit_mutation(
    state: &AppState,
    headers: &HeaderMap,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> Result<(), ApiError> {
    state
        .persist_audit(mutation_audit_event(
            headers,
            action,
            entity_type,
            entity_id,
            before,
            after,
            reason,
        ))
        .await
}

fn mutation_audit_event(
    headers: &HeaderMap,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> AuditEvent {
    AuditEvent {
        id: Uuid::new_v4(),
        actor: actor(headers),
        action: action.into(),
        entity_type: entity_type.into(),
        entity_id,
        before,
        after,
        reason,
        request_id: request_id(headers),
        occurred_at: Utc::now(),
    }
}

async fn insert_audit_event_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &AuditEvent,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
    )
    .bind(event.id)
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.entity_type)
    .bind(event.entity_id)
    .bind(&event.before)
    .bind(&event.after)
    .bind(&event.reason)
    .bind(event.request_id)
    .bind(event.occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_memory_audit_if_needed(
    state: &AppState,
    event: AuditEvent,
) -> Result<(), ApiError> {
    if state.pool.is_none() {
        state.persist_audit(event).await?;
    }
    Ok(())
}

fn normalize_news_ownership(
    production: bool,
    previous: Option<DataClass>,
    input: &mut NewsDraftInput,
) -> Result<(), ApiError> {
    if input.data_class != DataClass::DevelopmentFixture {
        return Ok(());
    }
    if previous.is_none() {
        return Err(ApiError::validation(BTreeMap::from([(
            "dataClass".into(),
            vec![
                "Development fixtures can only be created by the development-only seed command."
                    .into(),
            ],
        )])));
    }
    if !input.content.is_placeholder {
        // Clearing the placeholder flag is the explicit Admin takeover signal.
        input.data_class = DataClass::Editorial;
        return Ok(());
    }
    if previous == Some(DataClass::Editorial) {
        return Err(ApiError::validation(BTreeMap::from([(
            "dataClass".into(),
            vec!["Editorial News cannot be reassigned to development fixture ownership.".into()],
        )])));
    }
    if production {
        return Err(ApiError::validation(BTreeMap::from([(
            "dataClass".into(),
            vec!["Development fixture News cannot be written by the production Admin API.".into()],
        )])));
    }
    Ok(())
}

fn validate_news_input(input: &mut NewsDraftInput) -> Result<(), ApiError> {
    input.content.kind = ContentKind::News;
    super::admin::validate_content_input(&input.content)?;
    let mut errors = BTreeMap::new();
    if input.content.slug.is_empty()
        || input.content.slug.len() > 200
        || !input
            .content
            .slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        errors.insert(
            "content.slug".into(),
            vec!["Must be a lowercase URL slug.".into()],
        );
    }
    if input.content.title.trim().is_empty() || input.content.title.len() > 300 {
        errors.insert(
            "content.title".into(),
            vec!["Must contain 1 to 300 characters.".into()],
        );
    }
    if input.category.trim().is_empty() || input.category.len() > 80 {
        errors.insert(
            "category".into(),
            vec!["Must contain 1 to 80 characters.".into()],
        );
    }
    if input.author_display_name.trim().is_empty() || input.author_display_name.len() > 160 {
        errors.insert(
            "authorDisplayName".into(),
            vec!["Must contain 1 to 160 characters.".into()],
        );
    }
    if matches!(input.data_class, DataClass::Feishu | DataClass::VerifiedCsv) {
        errors.insert(
            "dataClass".into(),
            vec!["News may be editorial or a development fixture only.".into()],
        );
    }
    let expected_canonical = format!("/en/resources/news/{}", input.content.slug);
    if input.content.seo.canonical_path.as_deref() != Some(expected_canonical.as_str()) {
        errors.insert(
            "content.seo.canonicalPath".into(),
            vec![format!("Must exactly equal `{expected_canonical}`.")],
        );
    }
    if input.data_class == DataClass::DevelopmentFixture {
        input.content.is_placeholder = true;
        input.content.seo.indexable = false;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_general_information(input: &GeneralInformationDraftInput) -> Result<(), ApiError> {
    if !valid_locale(&input.locale) {
        return Err(ApiError::bad_request("locale is invalid."));
    }
    let object = input
        .payload
        .as_object()
        .ok_or_else(|| ApiError::bad_request("payload must be a JSON object."))?;
    if serde_json::to_vec(&input.payload)
        .map_err(|_| ApiError::bad_request("payload is not serializable."))?
        .len()
        > 256 * 1024
    {
        return Err(ApiError::bad_request("payload must not exceed 256 KiB."));
    }
    let allowed = [
        "brandName",
        "brandLine",
        "homePath",
        "footerStatement",
        "copyrightText",
        "defaultSeo",
        "organization",
        "navigationCta",
        "productCategories",
    ];
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    for unknown in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        errors.insert(
            format!("payload.{unknown}"),
            vec!["Unknown General Information field.".into()],
        );
    }
    for &required in &allowed[..7] {
        if !object.contains_key(required) {
            errors.insert(
                format!("payload.{required}"),
                vec!["This General Information field is required.".into()],
            );
        }
    }
    validate_required_text(
        object.get("brandName"),
        "payload.brandName",
        120,
        &mut errors,
    );
    validate_nullable_text(
        object.get("brandLine"),
        "payload.brandLine",
        300,
        &mut errors,
    );
    validate_nullable_text(
        object.get("footerStatement"),
        "payload.footerStatement",
        2_000,
        &mut errors,
    );
    validate_nullable_text(
        object.get("copyrightText"),
        "payload.copyrightText",
        500,
        &mut errors,
    );
    match object.get("homePath").and_then(Value::as_str) {
        Some(path) if safe_internal_path(path) && (path == "/en" || path.starts_with("/en/")) => {}
        _ => insert_validation(
            &mut errors,
            "payload.homePath",
            "Must be a safe English public path beginning with /en.",
        ),
    }
    validate_default_seo(object.get("defaultSeo"), &mut errors);
    validate_organization(object.get("organization"), &mut errors);
    validate_navigation_cta(object.get("navigationCta"), &mut errors);
    validate_product_categories(object.get("productCategories"), &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_default_seo(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(object) = value.and_then(Value::as_object) else {
        insert_validation(errors, "payload.defaultSeo", "Must be a JSON object.");
        return;
    };
    for unknown in object
        .keys()
        .filter(|key| !matches!(key.as_str(), "title" | "description"))
    {
        insert_validation(
            errors,
            &format!("payload.defaultSeo.{unknown}"),
            "Unknown SEO field.",
        );
    }
    validate_nullable_text(object.get("title"), "payload.defaultSeo.title", 300, errors);
    validate_nullable_text(
        object.get("description"),
        "payload.defaultSeo.description",
        1_000,
        errors,
    );
}

fn validate_organization(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(object) = value.and_then(Value::as_object) else {
        insert_validation(errors, "payload.organization", "Must be a JSON object.");
        return;
    };
    let allowed = [
        "name",
        "url",
        "logoUrl",
        "legalName",
        "salesEmail",
        "marketingEmail",
        "address",
        "socialLinks",
    ];
    for unknown in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        insert_validation(
            errors,
            &format!("payload.organization.{unknown}"),
            "Unknown organization field.",
        );
    }
    validate_required_text(object.get("name"), "payload.organization.name", 200, errors);
    validate_nullable_text(
        object.get("legalName"),
        "payload.organization.legalName",
        300,
        errors,
    );
    validate_nullable_text(
        object.get("address"),
        "payload.organization.address",
        1_000,
        errors,
    );
    for field in ["salesEmail", "marketingEmail"] {
        match object.get(field) {
            None | Some(Value::Null) => {}
            Some(Value::String(value))
                if value.len() <= 254
                    && !value.chars().any(char::is_control)
                    && value.split_once('@').is_some_and(|(local, domain)| {
                        !local.is_empty() && !domain.is_empty() && !domain.starts_with('.')
                    }) => {}
            _ => insert_validation(
                errors,
                &format!("payload.organization.{field}"),
                "Must be null or a valid email address.",
            ),
        }
    }
    match object.get("url") {
        None | Some(Value::Null) => {}
        Some(Value::String(value)) if safe_https_url(value) => {}
        _ => insert_validation(
            errors,
            "payload.organization.url",
            "Must be null or an HTTPS URL.",
        ),
    }
    match object.get("logoUrl") {
        None | Some(Value::Null) => {}
        Some(Value::String(value)) if safe_https_url(value) || safe_internal_path(value) => {}
        _ => insert_validation(
            errors,
            "payload.organization.logoUrl",
            "Must be null, an HTTPS URL, or an absolute internal path.",
        ),
    }
    let Some(links) = object.get("socialLinks") else {
        return;
    };
    let Some(links) = links.as_array().filter(|links| links.len() <= 20) else {
        insert_validation(
            errors,
            "payload.organization.socialLinks",
            "Must be an array containing at most 20 links.",
        );
        return;
    };
    for (index, link) in links.iter().enumerate() {
        let path = format!("payload.organization.socialLinks.{index}");
        let Some(link) = link.as_object() else {
            insert_validation(errors, &path, "Must be a JSON object.");
            continue;
        };
        if link.len() != 2 || !link.contains_key("label") || !link.contains_key("url") {
            insert_validation(
                errors,
                &path,
                "Only label and url are allowed and required.",
            );
        }
        validate_required_text(link.get("label"), &format!("{path}.label"), 120, errors);
        match link.get("url").and_then(Value::as_str) {
            Some(url) if safe_https_url(url) => {}
            _ => insert_validation(errors, &format!("{path}.url"), "Must be an HTTPS URL."),
        }
    }
}

fn validate_navigation_cta(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(value) = value else { return };
    if value.is_null() {
        return;
    }
    let Some(object) = value.as_object() else {
        insert_validation(
            errors,
            "payload.navigationCta",
            "Must be null or a JSON object.",
        );
        return;
    };
    if object.len() != 2 || !object.contains_key("label") || !object.contains_key("href") {
        insert_validation(
            errors,
            "payload.navigationCta",
            "Only label and href are allowed and required.",
        );
    }
    validate_required_text(
        object.get("label"),
        "payload.navigationCta.label",
        120,
        errors,
    );
    match object.get("href").and_then(Value::as_str) {
        Some(path) if safe_internal_path(path) && (path == "/en" || path.starts_with("/en/")) => {}
        _ => insert_validation(
            errors,
            "payload.navigationCta.href",
            "Must be a safe English public path beginning with /en.",
        ),
    }
}

fn validate_product_categories(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(value) = value else { return };
    let Some(categories) = value.as_array().filter(|items| items.len() <= 10) else {
        insert_validation(
            errors,
            "payload.productCategories",
            "Must be an array containing at most 10 category presentations.",
        );
        return;
    };
    let mut codes = HashSet::new();
    for (index, category) in categories.iter().enumerate() {
        let path = format!("payload.productCategories.{index}");
        let Some(category) = category.as_object() else {
            insert_validation(errors, &path, "Must be a JSON object.");
            continue;
        };
        if category.len() != 5
            || !["code", "slug", "name", "description", "sortOrder"]
                .iter()
                .all(|key| category.contains_key(*key))
        {
            insert_validation(
                errors,
                &path,
                "Only code, slug, name, description, and sortOrder are allowed and required.",
            );
        }
        match category.get("code").and_then(Value::as_str) {
            Some(code)
                if matches!(
                    code,
                    "centrifugal" | "axial" | "crossFlow" | "inlineDuct" | "motors"
                ) && codes.insert(code.to_owned()) => {}
            _ => insert_validation(
                errors,
                &format!("{path}.code"),
                "Must be a unique supported product-family code.",
            ),
        }
        match category.get("slug").and_then(Value::as_str) {
            Some(slug)
                if !slug.is_empty()
                    && slug.len() <= 120
                    && slug.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    }) => {}
            _ => insert_validation(errors, &format!("{path}.slug"), "Must be a URL slug."),
        }
        validate_required_text(category.get("name"), &format!("{path}.name"), 120, errors);
        validate_required_text(
            category.get("description"),
            &format!("{path}.description"),
            2_000,
            errors,
        );
        if !category
            .get("sortOrder")
            .and_then(Value::as_i64)
            .is_some_and(|value| (-10_000..=10_000).contains(&value))
        {
            insert_validation(
                errors,
                &format!("{path}.sortOrder"),
                "Must be an integer from -10000 to 10000.",
            );
        }
    }
}

fn validate_required_text(
    value: Option<&Value>,
    path: &str,
    max: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    match value.and_then(Value::as_str) {
        Some(value)
            if !value.trim().is_empty()
                && value.len() <= max
                && !value.chars().any(char::is_control) => {}
        _ => insert_validation(
            errors,
            path,
            &format!("Must be a non-empty string of at most {max} characters."),
        ),
    }
}

fn validate_nullable_text(
    value: Option<&Value>,
    path: &str,
    max: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    match value {
        None | Some(Value::Null) => {}
        Some(Value::String(value))
            if value.len() <= max && !value.chars().any(char::is_control) => {}
        _ => insert_validation(
            errors,
            path,
            &format!("Must be null or a string of at most {max} characters."),
        ),
    }
}

fn safe_internal_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 2_048
        && !value.starts_with("//")
        && !value.contains(['\\', '\r', '\n', '\0'])
        && !value.to_ascii_lowercase().contains("javascript:")
}

fn safe_https_url(value: &str) -> bool {
    value.starts_with("https://")
        && value.len() <= 2_048
        && value[8..].contains('.')
        && !value.chars().any(char::is_control)
}

fn insert_validation(errors: &mut BTreeMap<String, Vec<String>>, path: &str, message: &str) {
    errors
        .entry(path.to_owned())
        .or_default()
        .push(message.to_owned());
}

fn validate_product_presentation(input: &UpdateProductPresentation) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if !valid_locale(&input.locale) {
        errors.insert("locale".into(), vec!["Must be a valid locale tag.".into()]);
    }
    if input.slug.is_empty()
        || input.slug.len() > 200
        || !input
            .slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        errors.insert("slug".into(), vec!["Must be a lowercase URL slug.".into()]);
    }
    if input.title.trim().is_empty() || input.title.len() > 300 {
        errors.insert(
            "title".into(),
            vec!["Must contain 1 to 300 characters.".into()],
        );
    }
    if input
        .summary
        .as_ref()
        .is_some_and(|value| value.len() > 2_000)
    {
        errors.insert(
            "summary".into(),
            vec!["Must not exceed 2,000 characters.".into()],
        );
    }
    if input.seo.canonical_path.as_ref().is_some_and(|path| {
        !path.starts_with("/en/")
            || path.contains(['?', '#', '\\', '\r', '\n', '\0'])
            || path.starts_with("//")
            || path.len() > 2_048
    }) {
        errors.insert(
            "seo.canonicalPath".into(),
            vec!["Must be an English public path beginning with /en/.".into()],
        );
    }
    if !(-10_000..=10_000).contains(&input.sort_order) {
        errors.insert(
            "sortOrder".into(),
            vec!["Must be from -10000 to 10000.".into()],
        );
    }
    if input.related_content_ids.len() > 100
        || input
            .related_content_ids
            .iter()
            .collect::<HashSet<_>>()
            .len()
            != input.related_content_ids.len()
    {
        errors.insert(
            "relatedContentIds".into(),
            vec!["Must contain at most 100 unique content ids.".into()],
        );
    }
    validate_reason(&input.reason)?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_product_canonical(
    input: &UpdateProductPresentation,
    family: crate::models::ProductFamily,
) -> Result<(), ApiError> {
    let Some(canonical_path) = input.seo.canonical_path.as_deref() else {
        if input.indexable {
            return Err(ApiError::validation(BTreeMap::from([(
                "seo.canonicalPath".into(),
                vec!["An indexable product requires a canonical path.".into()],
            )])));
        }
        return Ok(());
    };
    let family = match family {
        crate::models::ProductFamily::Centrifugal => "centrifugal",
        crate::models::ProductFamily::Axial => "axial",
        crate::models::ProductFamily::CrossFlow => "cross-flow",
        crate::models::ProductFamily::InlineDuct => "inline-duct",
        crate::models::ProductFamily::Motors => "motors",
    };
    let expected = format!("/en/products/{family}/{}", input.slug);
    if canonical_path != expected {
        return Err(ApiError::validation(BTreeMap::from([(
            "seo.canonicalPath".into(),
            vec![format!("Must exactly match {expected}.")],
        )])));
    }
    Ok(())
}

const INVITATION_REPLAY_ENVELOPE_VERSION: u8 = 1;
const INVITATION_REPLAY_NONCE_BYTES: usize = 12;

fn invitation_replay_aad(headers: &HeaderMap, input: &InviteAdminUser) -> Result<String, ApiError> {
    let idempotency_key = parse_idempotency_key(headers)?;
    let key_hash = format!("{:x}", Sha256::digest(idempotency_key.as_bytes()));
    let input = serde_json::to_value(input)
        .map_err(|_| ApiError::internal("Invitation replay binding failed."))?;
    Ok(format!(
        "airtek.invitation-replay.v1|{key_hash}|{}",
        json_hash(&input)
    ))
}

fn seal_invitation_replay(
    invitation: &UserInvitation,
    encryption_key: &InvitationReplayEncryptionKey,
    aad: &[u8],
) -> Result<EncryptedInvitationReplay, ApiError> {
    let mut nonce = [0_u8; INVITATION_REPLAY_NONCE_BYTES];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| ApiError::internal("Secure invitation replay nonce generation failed."))?;
    let key = LessSafeKey::new(
        UnboundKey::new(&aead::AES_256_GCM, encryption_key.as_bytes())
            .map_err(|_| ApiError::internal("Invitation replay key initialization failed."))?,
    );
    let mut plaintext = serde_json::to_vec(invitation)
        .map_err(|_| ApiError::internal("Invitation replay serialization failed."))?;
    if key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad),
            &mut plaintext,
        )
        .is_err()
    {
        plaintext.zeroize();
        return Err(ApiError::internal("Invitation replay encryption failed."));
    }
    let envelope = EncryptedInvitationReplay {
        version: INVITATION_REPLAY_ENVELOPE_VERSION,
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        ciphertext: URL_SAFE_NO_PAD.encode(&plaintext),
    };
    plaintext.zeroize();
    Ok(envelope)
}

fn open_invitation_replay(
    envelope: &EncryptedInvitationReplay,
    encryption_key: &InvitationReplayEncryptionKey,
    aad: &[u8],
) -> Result<UserInvitation, ApiError> {
    if envelope.version != INVITATION_REPLAY_ENVELOPE_VERSION {
        return Err(ApiError::service_unavailable(
            "Stored invitation replay uses an unsupported envelope version.",
        ));
    }
    let nonce: [u8; INVITATION_REPLAY_NONCE_BYTES] = URL_SAFE_NO_PAD
        .decode(&envelope.nonce)
        .ok()
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| {
            ApiError::service_unavailable("Stored invitation replay nonce is invalid.")
        })?;
    let mut ciphertext = URL_SAFE_NO_PAD.decode(&envelope.ciphertext).map_err(|_| {
        ApiError::service_unavailable("Stored invitation replay ciphertext is invalid.")
    })?;
    if ciphertext.len() < aead::AES_256_GCM.tag_len() {
        ciphertext.zeroize();
        return Err(ApiError::service_unavailable(
            "Stored invitation replay ciphertext is invalid.",
        ));
    }
    let key = LessSafeKey::new(
        UnboundKey::new(&aead::AES_256_GCM, encryption_key.as_bytes()).map_err(|_| {
            ApiError::service_unavailable("Invitation replay key initialization failed.")
        })?,
    );
    let opened_length = match key.open_in_place(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad),
        &mut ciphertext,
    ) {
        Ok(opened) => opened.len(),
        Err(_) => {
            ciphertext.zeroize();
            return Err(ApiError::service_unavailable(
                "Stored invitation replay could not be authenticated.",
            ));
        }
    };
    let invitation = serde_json::from_slice::<UserInvitation>(&ciphertext[..opened_length])
        .map_err(|_| ApiError::service_unavailable("Stored invitation replay payload is invalid."));
    ciphertext.zeroize();
    let invitation = invitation?;
    if invitation
        .invitation_token
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Err(ApiError::service_unavailable(
            "Stored invitation replay token is missing.",
        ));
    }
    Ok(invitation)
}

fn validate_invitation(input: &InviteAdminUser) -> Result<(), ApiError> {
    if !input.email.contains('@') || input.email.len() > 254 {
        return Err(ApiError::bad_request("email is invalid."));
    }
    if input.display_name.trim().is_empty() || input.display_name.len() > 200 {
        return Err(ApiError::bad_request(
            "displayName must contain 1 to 200 characters.",
        ));
    }
    if input.role_keys.is_empty() || input.role_keys.len() > 20 {
        return Err(ApiError::bad_request(
            "roleKeys must contain 1 to 20 roles.",
        ));
    }
    Ok(())
}

fn validate_time_range(
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
) -> Result<(), ApiError> {
    if from.zip(to).is_some_and(|(from, to)| from >= to) {
        Err(ApiError::bad_request("from must be before to."))
    } else {
        Ok(())
    }
}

fn analytics_cursor_scope(
    endpoint: &str,
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
) -> String {
    let bound = |value: Option<chrono::DateTime<Utc>>| {
        value
            .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true))
            .unwrap_or_else(|| "*".into())
    };
    format!("{endpoint}|from={}|to={}", bound(from), bound(to))
}

fn decode_analytics_cursor(
    scope: &str,
    value: Option<&str>,
) -> Result<Option<AnalyticsCursor>, ApiError> {
    value
        .map(|value| decode_scoped_cursor::<AnalyticsCursor>(scope, value))
        .transpose()?
        .map(|cursor| {
            if cursor.dimension_hash.len() != 32 {
                Err(ApiError::bad_request("cursor is invalid."))
            } else {
                Ok(cursor)
            }
        })
        .transpose()
}

fn analytics_cursor_from_database_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AnalyticsCursor, ApiError> {
    let dimension_hash: Vec<u8> = row.try_get("dimension_hash")?;
    if dimension_hash.len() != 32 {
        return Err(ApiError::service_unavailable(
            "Stored analytics dimension identity is invalid.",
        ));
    }
    Ok(AnalyticsCursor {
        bucket_date: row.try_get("bucket_date")?,
        dimension_hash,
    })
}

fn ensure_no_analytics_hash_collision(count: i64) -> Result<(), ApiError> {
    if count == 1 {
        Ok(())
    } else {
        Err(ApiError::service_unavailable(
            "Analytics dimension identity collision detected.",
        ))
    }
}

fn analytics_dimension_hash(parts: &[&str]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize().to_vec()
}

fn analytics_key_is_after(value: &AnalyticsCursor, after: &AnalyticsCursor) -> bool {
    value.bucket_date < after.bucket_date
        || (value.bucket_date == after.bucket_date && value.dimension_hash > after.dimension_hash)
}

fn finish_analytics_page<T>(
    scope: &str,
    mut values: Vec<(T, AnalyticsCursor)>,
    limit: usize,
) -> Result<CursorPage<T>, ApiError> {
    let has_more = values.len() > limit;
    values.truncate(limit);
    let next_cursor = if has_more {
        values
            .last()
            .map(|(_, cursor)| encode_scoped_cursor(scope, cursor))
            .transpose()?
    } else {
        None
    };
    Ok(CursorPage {
        items: values.into_iter().map(|(value, _)| value).collect(),
        next_cursor,
    })
}

fn paginate_memory_analytics<T>(
    scope: &str,
    mut values: Vec<(T, AnalyticsCursor)>,
    after: Option<&AnalyticsCursor>,
    limit: usize,
) -> Result<CursorPage<T>, ApiError> {
    let mut identities = HashSet::with_capacity(values.len());
    if values
        .iter()
        .any(|(_, cursor)| !identities.insert(cursor.clone()))
    {
        return Err(ApiError::service_unavailable(
            "Analytics dimension identity collision detected.",
        ));
    }
    values.sort_by(|(_, left), (_, right)| {
        right
            .bucket_date
            .cmp(&left.bucket_date)
            .then_with(|| left.dimension_hash.cmp(&right.dimension_hash))
    });
    if let Some(after) = after {
        values.retain(|(_, cursor)| analytics_key_is_after(cursor, after));
    }
    values.truncate(limit + 1);
    finish_analytics_page(scope, values, limit)
}

fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if reason.trim().len() < 10 || reason.len() > 1000 {
        Err(ApiError::bad_request(
            "reason must contain 10 to 1000 characters.",
        ))
    } else {
        Ok(())
    }
}

fn decode_publication_status(value: String) -> PublicationStatus {
    match value.as_str() {
        "scheduled" => PublicationStatus::Scheduled,
        "published" => PublicationStatus::Published,
        "archived" => PublicationStatus::Archived,
        _ => PublicationStatus::Draft,
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(value: Value, entity: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| {
        tracing::error!(%error, entity, "stored JSON payload is invalid");
        ApiError::service_unavailable(format!("Stored {entity} data is invalid."))
    })
}

fn entity_response<T: Serialize>(status: StatusCode, value: &T, revision: i64) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
}

fn request_id(headers: &HeaderMap) -> Uuid {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

fn valid_locale(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn default_locale() -> String {
    "en".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        models::{ContentDraftInput, RichTextDocument, SeoMetadata},
    };

    #[test]
    fn clearing_a_development_news_placeholder_takes_editorial_ownership() {
        let mut input = NewsDraftInput {
            content: ContentDraftInput {
                kind: ContentKind::Article,
                slug: "development-news".into(),
                locale: "en".into(),
                title: "Development news".into(),
                summary: None,
                body: RichTextDocument {
                    schema_version: 1,
                    doc: json!({"type":"doc","content":[]}),
                },
                seo: SeoMetadata {
                    indexable: true,
                    canonical_path: Some("/en/resources/news/development-news".into()),
                    ..SeoMetadata::default()
                },
                is_placeholder: false,
            },
            category: "Company".into(),
            author_display_name: "AIRTEKPOWER".into(),
            cover_media_id: None,
            published_at: None,
            featured: false,
            data_class: DataClass::DevelopmentFixture,
        };
        normalize_news_ownership(false, Some(DataClass::DevelopmentFixture), &mut input).unwrap();
        validate_news_input(&mut input).unwrap();
        assert_eq!(input.content.kind, ContentKind::News);
        assert_eq!(input.data_class, DataClass::Editorial);
        assert!(!input.content.is_placeholder);
        assert!(input.content.seo.indexable);
    }

    #[test]
    fn admin_news_cannot_create_or_reclaim_development_fixture_ownership() {
        let fixture_input = || NewsDraftInput {
            content: ContentDraftInput {
                kind: ContentKind::News,
                slug: "fixture-ownership".into(),
                locale: "en".into(),
                title: "Fixture ownership".into(),
                summary: None,
                body: RichTextDocument {
                    schema_version: 1,
                    doc: json!({"type":"doc","content":[]}),
                },
                seo: SeoMetadata {
                    canonical_path: Some("/en/resources/news/fixture-ownership".into()),
                    indexable: false,
                    ..SeoMetadata::default()
                },
                is_placeholder: true,
            },
            category: "Company".into(),
            author_display_name: "AIRTEKPOWER".into(),
            cover_media_id: None,
            published_at: None,
            featured: false,
            data_class: DataClass::DevelopmentFixture,
        };

        assert!(normalize_news_ownership(false, None, &mut fixture_input()).is_err());
        assert!(
            normalize_news_ownership(false, Some(DataClass::Editorial), &mut fixture_input(),)
                .is_err()
        );
        assert!(normalize_news_ownership(
            true,
            Some(DataClass::DevelopmentFixture),
            &mut fixture_input(),
        )
        .is_err());
    }

    #[test]
    fn news_rejects_stored_script_content() {
        let mut input = NewsDraftInput {
            content: ContentDraftInput {
                kind: ContentKind::News,
                slug: "unsafe-news".into(),
                locale: "en".into(),
                title: "Unsafe news".into(),
                summary: None,
                body: RichTextDocument {
                    schema_version: 1,
                    doc: json!({"type":"doc","content":[{"type":"script","text":"alert(1)"}]}),
                },
                seo: SeoMetadata {
                    canonical_path: Some("/en/resources/news/unsafe-news".into()),
                    indexable: false,
                    ..SeoMetadata::default()
                },
                is_placeholder: false,
            },
            category: "Company".into(),
            author_display_name: "AIRTEKPOWER".into(),
            cover_media_id: None,
            published_at: None,
            featured: false,
            data_class: DataClass::Editorial,
        };
        assert!(validate_news_input(&mut input).is_err());
    }

    #[test]
    fn general_information_requires_frontend_contract_keys() {
        let input = GeneralInformationDraftInput {
            locale: "en".into(),
            payload: json!({"brandName":"AIRTEKPOWER"}),
            is_placeholder: true,
        };
        assert!(validate_general_information(&input).is_err());
    }

    #[test]
    fn invitation_replay_is_encrypted_and_bound_to_the_idempotent_request() {
        let config = Config::for_test();
        let key = config
            .invitation_replay_encryption_key
            .as_ref()
            .expect("test invitation replay key");
        let raw_token = "test-one-time-token-that-must-not-appear";
        let invitation = UserInvitation {
            id: Uuid::new_v4(),
            email: "invitee@example.com".into(),
            display_name: "Invitee".into(),
            locale: "zh-CN".into(),
            role_keys: vec!["content-editor".into()],
            status: "pending".into(),
            invited_at: Utc::now(),
            expires_at: Utc::now() + Duration::days(7),
            invitation_token: Some(raw_token.into()),
        };
        let aad = b"airtek.invitation-replay.v1|request-a";
        let envelope = seal_invitation_replay(&invitation, key, aad).unwrap();
        let stored = serde_json::to_string(&envelope).unwrap();
        assert!(!stored.contains(raw_token));
        assert_eq!(
            open_invitation_replay(&envelope, key, aad)
                .unwrap()
                .invitation_token
                .as_deref(),
            Some(raw_token)
        );
        assert!(open_invitation_replay(
            &envelope,
            key,
            b"airtek.invitation-replay.v1|different-request"
        )
        .is_err());
    }
}
