#[tokio::test]
async fn admin_settings_are_allowlisted_concurrent_and_audited() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/settings")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let settings = response_json(response).await;
    assert_eq!(settings["publicLocale"], "en");
    assert_eq!(settings["rfqRetentionDays"], 365);

    let request = |body: Value, etag: Option<&str>| {
        let mut request = Request::patch("/api/admin/v1/settings")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-request-id", "50000000-0000-4000-8000-000000000001")
            .body(Body::from(body.to_string()))
            .unwrap();
        if let Some(etag) = etag {
            request
                .headers_mut()
                .insert(header::IF_MATCH, etag.parse().unwrap());
        }
        request
    };

    let missing_precondition = app
        .clone()
        .oneshot(request(
            json!({
                "rfqRetentionDays": 400,
                "reason": "Increase retention for current policy"
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(
        missing_precondition.status(),
        StatusCode::PRECONDITION_REQUIRED
    );

    for body in [
        json!({
            "publicLocale": "fr",
            "reason": "Attempt to change a read-only value"
        }),
        json!({
            "rfqRetentionDays": 400,
            "deploymentSecret": "must-not-be-writable",
            "reason": "Attempt to write deployment policy"
        }),
    ] {
        let rejected = app
            .clone()
            .oneshot(request(body, Some("\"revision-1\"")))
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    }

    let invalid = app
        .clone()
        .oneshot(request(
            json!({"rfqRetentionDays": 29, "reason": "too short"}),
            Some("\"revision-1\""),
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let updated = app
        .clone()
        .oneshot(request(
            json!({
                "rfqRetentionDays": 400,
                "retentionDeletionGraceDays": 45,
                "temporaryOverrideDefaultDays": 21,
                "reason": "Align retention with approved policy"
            }),
            Some("\"revision-1\""),
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        updated.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let updated_body = response_json(updated).await;
    assert_eq!(updated_body["rfqRetentionDays"], 400);
    assert_eq!(updated_body["retentionDeletionGraceDays"], 45);
    assert_eq!(updated_body["temporaryOverrideDefaultDays"], 21);
    assert_eq!(updated_body["publicLocale"], "en");
    assert_eq!(updated_body["revision"], 2);

    let stale = app
        .clone()
        .oneshot(request(
            json!({
                "rfqRetentionDays": 401,
                "reason": "Stale concurrent policy change"
            }),
            Some("\"revision-1\""),
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        state
            .integer_setting("rfqRetentionDays", 365, 30, 3_650)
            .await
            .unwrap(),
        400
    );
    let data = state.data.read().await;
    let audit = data
        .audit_events
        .iter()
        .find(|event| event.action == "settings.update")
        .expect("settings audit event");
    assert_eq!(audit.actor, "admin@example.com");
    assert_eq!(
        audit.reason.as_deref(),
        Some("Align retention with approved policy")
    );
    assert_eq!(
        audit.request_id,
        uuid::Uuid::parse_str("50000000-0000-4000-8000-000000000001").unwrap()
    );
    assert_eq!(audit.before.as_ref().unwrap()["revision"], 1);
    assert_eq!(audit.after.as_ref().unwrap()["revision"], 2);
}

fn public_contact_request(index: usize, peer: SocketAddr, forwarded_for: &str) -> Request<Body> {
    let mut request = Request::post("/api/public/v1/contact")
        .header(header::CONTENT_TYPE, "application/json")
        .header("idempotency-key", format!("contact-rate-{index:04}"))
        .header("x-forwarded-for", forwarded_for)
        .body(Body::from(
            json!({
                "contact": {"name": "Buyer", "email": "buyer@example.com"},
                "topic": "General inquiry",
                "message": "Please contact our engineering procurement team.",
                "sourcePath": "/en/company/contact",
                "locale": "en",
                "consent": true
            })
            .to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(peer));
    request
}

#[tokio::test]
async fn untrusted_peer_cannot_bypass_public_limit_with_forwarded_headers() {
    let app = build_router(AppState::for_test());
    let peer: SocketAddr = "198.51.100.20:41000".parse().unwrap();
    for index in 0..5 {
        let response = app
            .clone()
            .oneshot(public_contact_request(
                index,
                peer,
                &format!("203.0.113.{}", index + 1),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    let blocked = app
        .oneshot(public_contact_request(5, peer, "203.0.113.250"))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        blocked.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
}

#[tokio::test]
async fn configured_proxy_uses_the_nearest_untrusted_forwarded_client() {
    let mut config = Config::for_test();
    config.trusted_proxy_cidrs = vec!["172.28.0.10/32".parse().unwrap()];
    let app = build_router(AppState::new(config).unwrap());
    let gateway: SocketAddr = "172.28.0.10:41000".parse().unwrap();
    for index in 0..6 {
        let response = app
            .clone()
            .oneshot(public_contact_request(
                index,
                gateway,
                &format!("203.0.113.{}", index + 1),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}

#[tokio::test]
async fn rfq_and_analytics_routes_enforce_their_public_limits() {
    let rfq_app = build_router(AppState::for_test());
    let rfq_body = json!({
        "journey": "selection",
        "contact": {"name": "Buyer", "email": "buyer@example.com"},
        "sourcePath": "/en/request-a-quote/selection",
        "locale": "en",
        "consent": true,
        "context": {
            "application": "Industrial cooling",
            "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
        }
    })
    .to_string();
    for index in 0..6 {
        let response = rfq_app
            .clone()
            .oneshot(
                Request::post("/api/public/v1/rfqs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", format!("rfq-rate-{index:04}"))
                    .body(Body::from(rfq_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let expected = if index < 5 {
            StatusCode::CREATED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(response.status(), expected);
    }

    let analytics_app = build_router(AppState::for_test());
    let analytics_body = json!({
        "eventName": "pageView",
        "sourcePath": "/en",
        "locale": "en",
        "consentGranted": false,
        "properties": {}
    })
    .to_string();
    for index in 0..=120 {
        let response = analytics_app
            .clone()
            .oneshot(
                Request::post("/api/public/v1/analytics/events")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(analytics_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let expected = if index < 120 {
            StatusCode::ACCEPTED
        } else {
            StatusCode::TOO_MANY_REQUESTS
        };
        assert_eq!(response.status(), expected);
    }
}
