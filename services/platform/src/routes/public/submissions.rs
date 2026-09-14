async fn create_rfq(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<CreateRfqRequest>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        RFQ_POLICY,
    )
    .await?;
    let idempotency = match begin_idempotency(&state, "public.rfq", &headers, &request).await? {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let response: AcceptedResponse = replay.decode()?;
            return Ok((status, Json(response)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_rfq(&request)?;
    validate_product_rfq_context(&state, &request).await?;
    let retention_days = state
        .integer_setting("rfqRetentionDays", 365, 30, 3_650)
        .await?;

    let submitted_at = Utc::now();
    let id = Uuid::new_v4();
    let submission = RfqSubmission {
        id,
        reference: reference("RFQ", id, submitted_at),
        request,
        status: "new".into(),
        submitted_at,
        retention_until: submitted_at + Duration::days(retention_days),
    };
    state.persist_rfq(&submission).await?;
    if state.pool.is_none() {
        state.data.write().await.rfqs.insert(id, submission.clone());
    }
    let accepted = AcceptedResponse {
        id,
        reference: submission.reference,
        accepted_at: submitted_at,
    };
    idempotency
        .complete(&state, &accepted, StatusCode::CREATED)
        .await?;
    Ok((StatusCode::CREATED, Json(accepted)).into_response())
}

async fn validate_product_rfq_context(
    state: &AppState,
    request: &CreateRfqRequest,
) -> Result<(), ApiError> {
    if request.journey != RfqJourney::Product {
        return Ok(());
    }
    let context = request
        .product_context
        .as_ref()
        .expect("structural RFQ validation requires product context");
    let published = if let Some(pool) = &state.pool {
        load_published_product_rows(pool, None, None, None)
            .await?
            .into_iter()
            .find(|product| product.id == context.product_id)
    } else {
        state
            .data
            .read()
            .await
            .published_products
            .get(&context.product_id)
            .cloned()
    }
    .ok_or_else(|| {
        ApiError::conflict(
            "The referenced product is not currently published; use Selection RFQ instead.",
        )
    })?;
    if published.stable_id != context.stable_id
        || published.model != context.model
        || published.published_revision != Some(context.published_revision)
    {
        return Err(ApiError::conflict(
            "Product RFQ context does not match the current immutable published product revision.",
        ));
    }
    Ok(())
}

async fn create_contact(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<CreateContactRequest>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        CONTACT_POLICY,
    )
    .await?;
    let idempotency = match begin_idempotency(&state, "public.contact", &headers, &request).await? {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let response: AcceptedResponse = replay.decode()?;
            return Ok((status, Json(response)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    validate_contact(&request)?;
    let retention_days = state
        .integer_setting("rfqRetentionDays", 365, 30, 3_650)
        .await?;

    let submitted_at = Utc::now();
    let id = Uuid::new_v4();
    let contact = ContactRequest {
        id,
        reference: reference("CONTACT", id, submitted_at),
        request,
        status: "new".into(),
        submitted_at,
        retention_until: submitted_at + Duration::days(retention_days),
    };
    state.persist_contact(&contact).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .contacts
            .insert(id, contact.clone());
    }
    let accepted = AcceptedResponse {
        id,
        reference: contact.reference,
        accepted_at: submitted_at,
    };
    idempotency
        .complete(&state, &accepted, StatusCode::CREATED)
        .await?;
    Ok((StatusCode::CREATED, Json(accepted)).into_response())
}

async fn create_analytics_consent(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(request): Json<CreateAnalyticsConsent>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        ANALYTICS_CONSENT_POLICY,
    )
    .await?;
    if request.policy_version != ANALYTICS_POLICY_VERSION {
        return Err(ApiError::validation(BTreeMap::from([(
            "policyVersion".into(),
            vec!["The analytics policy version is not current.".into()],
        )])));
    }

    let granted_at = Utc::now();
    let receipt = AnalyticsConsentReceipt {
        consent_receipt: Uuid::new_v4(),
        anonymous_session_id: request.anonymous_session_id,
        policy_version: request.policy_version,
        analytics_allowed: request.analytics_allowed,
        granted_at,
        expires_at: granted_at + Duration::days(ANALYTICS_CONSENT_LIFETIME_DAYS),
    };
    state.persist_analytics_consent(&receipt).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .analytics_consents
            .insert(receipt.consent_receipt, receipt.clone());
    }

    let mut response = (StatusCode::CREATED, Json(receipt)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}

async fn create_analytics_event(
    State(state): State<AppState>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    Json(event): Json<CreateAnalyticsEvent>,
) -> Result<Response, ApiError> {
    enforce_public_rate_limit(
        &state,
        &headers,
        peer.map(|Extension(ConnectInfo(address))| address.ip()),
        ANALYTICS_POLICY,
    )
    .await?;
    let idempotency = match begin_idempotency(
        &state,
        "public.analytics.event.create",
        &headers,
        &event,
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let receipt: AnalyticsEventReceipt = replay.decode()?;
            return Ok((status, Json(receipt)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if !event.consent_granted {
        let receipt = AnalyticsEventReceipt {
            accepted: false,
            event_id: None,
        };
        idempotency
            .complete(&state, &receipt, StatusCode::ACCEPTED)
            .await?;
        return Ok((StatusCode::ACCEPTED, Json(receipt)).into_response());
    }
    validate_analytics_event(&event)?;
    validate_analytics_consent(&state, &event).await?;
    let event_id = Uuid::new_v4();
    let occurred_at = Utc::now();
    let event_value = serde_json::to_value(&event)
        .map_err(|_| ApiError::internal("Analytics serialization failed."))?;
    state
        .persist_analytics_event(event_id, &event_value, occurred_at)
        .await?;
    let receipt = AnalyticsEventReceipt {
        accepted: true,
        event_id: Some(event_id),
    };
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .analytics_receipts
            .insert(event_id, receipt.clone());
    }
    idempotency
        .complete(&state, &receipt, StatusCode::ACCEPTED)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(receipt)).into_response())
}

async fn validate_analytics_consent(
    state: &AppState,
    event: &CreateAnalyticsEvent,
) -> Result<(), ApiError> {
    let Some(anonymous_session_id) = event.anonymous_session_id else {
        return Err(ApiError::validation(BTreeMap::from([(
            "anonymousSessionId".into(),
            vec!["A consented analytics event requires an anonymous session ID.".into()],
        )])));
    };
    let Some(policy_version) = event.policy_version.as_deref() else {
        return Err(ApiError::validation(BTreeMap::from([(
            "policyVersion".into(),
            vec!["A consented analytics event requires a policy version.".into()],
        )])));
    };
    let Some(consent_receipt) = event.consent_receipt else {
        return Err(ApiError::validation(BTreeMap::from([(
            "consentReceipt".into(),
            vec!["A consented analytics event requires a consent receipt.".into()],
        )])));
    };
    let receipt = state
        .current_analytics_consent(consent_receipt)
        .await?
        .filter(|receipt| {
            receipt.analytics_allowed
                && receipt.expires_at > Utc::now()
                && state
                    .analytics_storage_session_id(anonymous_session_id)
                    .is_ok_and(|storage_id| receipt.anonymous_session_id == storage_id)
                && receipt.policy_version == policy_version
                && policy_version == ANALYTICS_POLICY_VERSION
        });
    if receipt.is_none() {
        return Err(ApiError::validation(BTreeMap::from([(
            "consentReceipt".into(),
            vec!["The consent receipt is invalid, expired, denied, superseded, or does not match this session and policy.".into()],
        )])));
    }
    Ok(())
}
