use super::*;

pub(super) async fn list_news(
    State(state): State<AppState>,
    Query(query): Query<NewsQuery>,
) -> Result<Json<CursorPage<NewsEntry>>, ApiError> {
    #[derive(Deserialize, Serialize)]
    struct Position {
        published_at: DateTime<Utc>,
        id: Uuid,
    }
    validate_locale(&query.locale)?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let scope = format!("public.news|{}|{:?}", query.locale, query.category);
    let after = match query.cursor.as_deref() {
        Some(cursor) => match decode_scoped_cursor::<Position>(&scope, cursor) {
            Ok(position) => Some(position),
            Err(_) => {
                let id = Uuid::parse_str(cursor)
                    .map_err(|_| ApiError::bad_request("cursor is invalid."))?;
                let published_at = resolve_legacy_news_cursor(
                    &state,
                    &query.locale,
                    query.category.as_deref(),
                    id,
                )
                .await?
                .ok_or_else(|| {
                    ApiError::bad_request("cursor is stale or belongs to other filters.")
                })?;
                state.request_metrics.record_legacy_cursor(
                    crate::services::request_metrics::LegacyCursorEndpoint::PublicNews,
                );
                Some(Position { published_at, id })
            }
        },
        None => None,
    };
    let mut values = load_published_news(
        &state,
        &query.locale,
        query.category.as_deref(),
        None,
        after.map(|value| (value.published_at, value.id)),
        limit + 1,
    )
    .await?;
    let has_more = values.len() > limit;
    values.truncate(limit);
    let next_cursor = if has_more {
        values
            .last()
            .map(|entry| {
                encode_scoped_cursor(
                    &scope,
                    &Position {
                        published_at: entry.published_at.unwrap_or(entry.content.updated_at),
                        id: entry.content.id,
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(Json(CursorPage {
        items: values,
        next_cursor,
    }))
}

pub(super) async fn get_news(
    State(state): State<AppState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Query(query): Query<LocaleQuery>,
) -> Result<Response, ApiError> {
    validate_locale(&query.locale)?;
    let news = load_published_news(&state, &query.locale, None, Some(&slug), None, 2)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found("Published news was not found."))?;
    let revision = news.content.published_revision;
    let mut response = Json(news).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    Ok(response)
}

pub(super) async fn create_guest_visit(
    State(state): State<AppState>,
    Json(request): Json<CreateGuestVisit>,
) -> Result<Response, ApiError> {
    validate_guest_visit(&state.config, &request)?;
    let request = normalize_guest_visit(request);
    let storage_session_id = state.analytics_storage_session_id(request.anonymous_session_id)?;
    let receipt = state
        .current_analytics_consent(request.consent_receipt)
        .await?
        .filter(|receipt| {
            receipt.analytics_allowed
                && receipt.expires_at > Utc::now()
                && receipt.anonymous_session_id == storage_session_id
                && receipt.policy_version == request.policy_version
                && request.policy_version == crate::routes::public::ANALYTICS_POLICY_VERSION
        })
        .ok_or_else(|| {
            ApiError::validation(BTreeMap::from([(
                "consentReceipt".into(),
                vec!["An active affirmative analytics consent receipt is required.".into()],
            )]))
        })?;
    let now = Utc::now();
    let source = classify_source(&request);
    let locale = request
        .landing_path
        .trim_start_matches('/')
        .split('/')
        .next()
        .filter(|value| valid_locale_tag(value))
        .unwrap_or("en")
        .to_owned();

    let (visit, created) = crate::services::public_content::upsert_guest_visit(
        &state.pool,
        crate::services::public_content::GuestVisitWrite {
            storage_session_id,
            browser_session_id: request.anonymous_session_id,
            consent_receipt: receipt.consent_receipt,
            locale: &locale,
            landing_path: &request.landing_path,
            source: &source,
            referrer_domain: request.referrer_domain.as_deref(),
            utm_source: request.source.as_deref(),
            medium: request.medium.as_deref(),
            campaign: request.campaign.as_deref(),
            now,
            retention_until: now + Duration::days(state.config.guest_raw_retention_days),
        },
    )
    .await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let mut response = (status, Json(visit)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}
