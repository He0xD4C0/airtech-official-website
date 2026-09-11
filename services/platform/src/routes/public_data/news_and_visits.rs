async fn list_news(
    State(state): State<AppState>,
    Query(query): Query<NewsQuery>,
) -> Result<Json<CursorPage<NewsEntry>>, ApiError> {
    validate_locale(&query.locale)?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let after = query
        .cursor
        .as_deref()
        .map(|cursor| {
            Uuid::parse_str(cursor).map_err(|_| ApiError::bad_request("cursor is invalid."))
        })
        .transpose()?;

    let mut values = load_published_news(&state, &query.locale, query.category.as_deref()).await?;
    values.sort_by(|left, right| {
        right
            .published_at
            .unwrap_or(right.content.updated_at)
            .cmp(&left.published_at.unwrap_or(left.content.updated_at))
            .then_with(|| left.content.id.cmp(&right.content.id))
    });
    if let Some(after) = after {
        let position = values
            .iter()
            .position(|entry| entry.content.id == after)
            .ok_or_else(|| ApiError::bad_request("cursor is stale or belongs to other filters."))?;
        values.drain(..=position);
    }
    let has_more = values.len() > limit;
    values.truncate(limit);
    let next_cursor = has_more
        .then(|| values.last().map(|entry| entry.content.id.to_string()))
        .flatten();
    Ok(Json(CursorPage {
        items: values,
        next_cursor,
    }))
}

async fn get_news(
    State(state): State<AppState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Query(query): Query<LocaleQuery>,
) -> Result<Response, ApiError> {
    validate_locale(&query.locale)?;
    let news = load_published_news(&state, &query.locale, None)
        .await?
        .into_iter()
        .find(|entry| entry.content.slug.as_deref() == Some(slug.as_str()))
        .ok_or_else(|| ApiError::not_found("Published news was not found."))?;
    let revision = news.content.published_revision;
    let mut response = Json(news).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    Ok(response)
}

async fn create_guest_visit(
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
                && request.policy_version == super::public::ANALYTICS_POLICY_VERSION
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

    let (visit, created) = if let Some(pool) = &state.pool {
        let existing_id = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM guest_visits
               WHERE anonymous_session_id=$1 AND consent_record_id=$2 AND retention_until > now()
               ORDER BY first_seen_at DESC LIMIT 1"#,
        )
        .bind(storage_session_id)
        .bind(receipt.consent_receipt)
        .fetch_optional(pool)
        .await?;
        let id = existing_id.unwrap_or_else(Uuid::new_v4);
        let retention_until = now + Duration::days(state.config.guest_raw_retention_days);
        let row = sqlx::query(
            r#"INSERT INTO guest_visits
               (id, anonymous_session_id, consent_record_id, consent_analytics_allowed,
                locale, landing_path, source_type, referrer_host, utm_source, utm_medium,
                utm_campaign, first_seen_at, last_seen_at, retention_until, created_at)
               VALUES ($1,$2,$3,true,$4,$5,$6,$7,$8,$9,$10,$11,$11,$12,$11)
               ON CONFLICT (id) DO UPDATE SET last_seen_at=EXCLUDED.last_seen_at
               RETURNING id, anonymous_session_id, landing_path, referrer_host, source_type,
                         utm_medium, utm_campaign, first_seen_at, last_seen_at, retention_until"#,
        )
        .bind(id)
        .bind(storage_session_id)
        .bind(receipt.consent_receipt)
        .bind(&locale)
        .bind(&request.landing_path)
        .bind(&source)
        .bind(request.referrer_domain.as_deref())
        .bind(request.source.as_deref())
        .bind(request.medium.as_deref())
        .bind(request.campaign.as_deref())
        .bind(now)
        .bind(retention_until)
        .fetch_one(pool)
        .await?;
        let mut visit = decode_guest_visit(&row)?;
        // The keyed database identifier is never returned to the browser.
        visit.anonymous_session_id = request.anonymous_session_id;
        (visit, existing_id.is_none())
    } else {
        let existing = {
            let data = state.data.read().await;
            data.guest_visits
                .values()
                .find(|visit| {
                    visit.anonymous_session_id == request.anonymous_session_id
                        && data.guest_visit_consent_records.get(&visit.id)
                            == Some(&receipt.consent_receipt)
                        && visit.retention_until > now
                })
                .cloned()
        };
        let created = existing.is_none();
        let visit = match existing {
            Some(mut visit) => {
                visit.last_seen_at = now;
                visit
            }
            None => GuestVisit {
                id: Uuid::new_v4(),
                anonymous_session_id: request.anonymous_session_id,
                landing_path: request.landing_path,
                referrer_domain: request.referrer_domain,
                source,
                medium: request.medium,
                campaign: request.campaign,
                first_seen_at: now,
                last_seen_at: now,
                retention_until: now + Duration::days(state.config.guest_raw_retention_days),
            },
        };
        let mut data = state.data.write().await;
        data.guest_visit_consent_records
            .insert(visit.id, receipt.consent_receipt);
        data.guest_visits.insert(visit.id, visit.clone());
        (visit, created)
    };
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
