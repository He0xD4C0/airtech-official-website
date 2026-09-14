#[tokio::test]
async fn analytics_requires_a_current_server_receipt_and_strict_event_dictionary() {
    let app = build_router(AppState::for_test());
    let session_id = uuid::Uuid::new_v4();
    let consent = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/consents")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "anonymousSessionId": session_id,
                        "policyVersion": "analytics-v1",
                        "analyticsAllowed": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(consent.status(), StatusCode::CREATED);
    assert_eq!(
        consent.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let consent = response_json(consent).await;
    let receipt = consent["consentReceipt"].as_str().unwrap();

    let visit = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/guest-visits")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "anonymousSessionId": session_id,
                        "consentReceipt": receipt,
                        "policyVersion": "analytics-v1",
                        "landingPath": "/en/products"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(visit.status(), StatusCode::CREATED);

    let event = |event_name: &str, properties: Value, session: uuid::Uuid, receipt: &str| {
        json!({
            "eventName": event_name,
            "anonymousSessionId": session,
            "sourcePath": "/en/products",
            "locale": "en",
            "consentGranted": true,
            "policyVersion": "analytics-v1",
            "consentReceipt": receipt,
            "properties": properties
        })
        .to_string()
    };
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header("idempotency-key", format!("analytics-event-{}", uuid::Uuid::new_v4()))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(event(
                    "filterApplied",
                    json!({"filterName": "family", "resultCount": 2}),
                    session_id,
                    receipt,
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(response).await["accepted"], true);

    for (event_name, properties) in [
        ("inventedEvent", json!({})),
        ("pageView", json!({"email": "buyer@example.com"})),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": {"nested": true}}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "buyer@example.com"}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "+1 (555) 010-1234"}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "x".repeat(161)}),
        ),
        (
            "filterApplied",
            json!({"filterName": "family", "filterValue": "Jane Doe"}),
        ),
        (
            "faqExpanded",
            json!({"faqId": "faq-1", "category": "arbitrary-free-text"}),
        ),
        ("pageView", json!({"contentKind": "Jane Doe"})),
        (
            "filterApplied",
            json!({"filterName": "jane-doe", "resultCount": 2}),
        ),
        ("faqExpanded", json!({"faqId": "jane-doe"})),
        (
            "ctaClicked",
            json!({"ctaId": "private-note", "placement": "hero"}),
        ),
        (
            "ctaClicked",
            json!({"ctaId": "request-quote", "placement": "jane-doe"}),
        ),
        (
            "ctaClicked",
            json!({"ctaId": "request-quote", "placement": "hero", "destinationPath": "/en/jane-doe"}),
        ),
        (
            "rfqValidationError",
            json!({
                "journey": "product",
                "step": 1,
                "fieldName": "jane-doe",
                "errorCode": "publishedContextRequired"
            }),
        ),
        (
            "rfqSubmitFailed",
            json!({"journey": "product", "errorCode": "jane-doe"}),
        ),
    ] {
        let rejected = app
            .clone()
            .oneshot(
                Request::post("/api/public/v1/analytics/events")
                .header("idempotency-key", format!("analytics-event-{}", uuid::Uuid::new_v4()))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(event(
                        event_name, properties, session_id, receipt,
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    let missing_receipt = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header("idempotency-key", format!("analytics-event-{}", uuid::Uuid::new_v4()))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "eventName": "pageView",
                        "anonymousSessionId": session_id,
                        "sourcePath": "/en",
                        "locale": "en",
                        "consentGranted": true,
                        "policyVersion": "analytics-v1",
                        "properties": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_receipt.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let mismatched_session = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header("idempotency-key", format!("analytics-event-{}", uuid::Uuid::new_v4()))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(event(
                    "pageView",
                    json!({}),
                    uuid::Uuid::new_v4(),
                    receipt,
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        mismatched_session.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn analytics_denial_supersedes_an_existing_allow_receipt() {
    let app = build_router(AppState::for_test());
    let session_id = uuid::Uuid::new_v4();
    let decision = |allowed: bool| {
        Request::post("/api/public/v1/analytics/consents")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "anonymousSessionId": session_id,
                    "policyVersion": "analytics-v1",
                    "analyticsAllowed": allowed
                })
                .to_string(),
            ))
            .unwrap()
    };
    let allowed = app.clone().oneshot(decision(true)).await.unwrap();
    let allowed = response_json(allowed).await;
    let receipt = allowed["consentReceipt"].as_str().unwrap();
    let denied = app.clone().oneshot(decision(false)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::CREATED);

    let event = app
        .oneshot(
            Request::post("/api/public/v1/analytics/events")
                .header("idempotency-key", format!("analytics-event-{}", uuid::Uuid::new_v4()))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "eventName": "pageView",
                        "anonymousSessionId": session_id,
                        "sourcePath": "/en",
                        "locale": "en",
                        "consentGranted": true,
                        "policyVersion": "analytics-v1",
                        "consentReceipt": receipt,
                        "properties": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(event.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn in_memory_analytics_requires_a_visit_for_the_current_consent_receipt() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session_id = uuid::Uuid::new_v4();
    let create_consent = || {
        Request::post("/api/public/v1/analytics/consents")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "anonymousSessionId": session_id,
                    "policyVersion": "analytics-v1",
                    "analyticsAllowed": true
                })
                .to_string(),
            ))
            .unwrap()
    };
    let create_visit = |receipt: &str| {
        Request::post("/api/public/v1/guest-visits")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "anonymousSessionId": session_id,
                    "consentReceipt": receipt,
                    "policyVersion": "analytics-v1",
                    "landingPath": "/en"
                })
                .to_string(),
            ))
            .unwrap()
    };
    let create_event = |receipt: &str| {
        Request::post("/api/public/v1/analytics/events")
                .header("idempotency-key", format!("analytics-event-{}", uuid::Uuid::new_v4()))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "eventName": "pageView",
                    "anonymousSessionId": session_id,
                    "sourcePath": "/en",
                    "locale": "en",
                    "consentGranted": true,
                    "policyVersion": "analytics-v1",
                    "consentReceipt": receipt,
                    "properties": {}
                })
                .to_string(),
            ))
            .unwrap()
    };

    let first_consent = response_json(app.clone().oneshot(create_consent()).await.unwrap()).await;
    let first_receipt = first_consent["consentReceipt"].as_str().unwrap();
    assert_eq!(
        app.clone()
            .oneshot(create_visit(first_receipt))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );

    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    let current_consent = response_json(app.clone().oneshot(create_consent()).await.unwrap()).await;
    let current_receipt = current_consent["consentReceipt"].as_str().unwrap();

    assert_eq!(
        app.clone()
            .oneshot(create_event(current_receipt))
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "a current receipt must not reuse the visit created for an older consent"
    );
    assert_eq!(
        app.clone()
            .oneshot(create_visit(current_receipt))
            .await
            .unwrap()
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        app.oneshot(create_event(current_receipt))
            .await
            .unwrap()
            .status(),
        StatusCode::ACCEPTED
    );

    let data = state.data.read().await;
    assert_eq!(data.guest_visits.len(), 2);
    assert_eq!(data.guest_visit_consent_records.len(), 2);
}

#[tokio::test]
async fn analytics_event_idempotency_replays_without_duplicate_counting() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session_id = uuid::Uuid::new_v4();
    let consent = app.clone().oneshot(
        Request::post("/api/public/v1/analytics/consents")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({
                "anonymousSessionId": session_id,
                "policyVersion": "analytics-v1",
                "analyticsAllowed": true
            }).to_string())).unwrap(),
    ).await.unwrap();
    let consent = response_json(consent).await;
    let receipt = consent["consentReceipt"].as_str().unwrap();
    let visit = app.clone().oneshot(
        Request::post("/api/public/v1/guest-visits")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({
                "anonymousSessionId": session_id,
                "consentReceipt": receipt,
                "policyVersion": "analytics-v1",
                "landingPath": "/en"
            }).to_string())).unwrap(),
    ).await.unwrap();
    assert_eq!(visit.status(), StatusCode::CREATED);

    let event = |source_path: &str| Request::post("/api/public/v1/analytics/events")
        .header("idempotency-key", "analytics-replay-contract-0001")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({
            "eventName": "pageView",
            "anonymousSessionId": session_id,
            "sourcePath": source_path,
            "locale": "en",
            "consentGranted": true,
            "policyVersion": "analytics-v1",
            "consentReceipt": receipt,
            "properties": {}
        }).to_string())).unwrap();
    let first = app.clone().oneshot(event("/en")).await.unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first = response_json(first).await;
    let replay = app.clone().oneshot(event("/en")).await.unwrap();
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(replay).await, first);
    let conflict = app.oneshot(event("/en/products")).await.unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(state.data.read().await.analytics_events.len(), 1);
}

#[tokio::test]
async fn analytics_consent_rejects_a_client_selected_policy_version() {
    let app = build_router(AppState::for_test());
    let response = app
        .oneshot(
            Request::post("/api/public/v1/analytics/consents")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "anonymousSessionId": uuid::Uuid::new_v4(),
                        "policyVersion": "client-invented-policy",
                        "analyticsAllowed": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
