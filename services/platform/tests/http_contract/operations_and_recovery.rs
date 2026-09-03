#[tokio::test]
async fn in_memory_feishu_sync_fails_explicitly_without_a_provider() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let started = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/feishu/sync-runs")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "feishu-sync-contract-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"dryRun": true, "mappingVersion": "validated-mapping-v1"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(started.status(), StatusCode::ACCEPTED);
    let started = response_json(started).await;
    assert_eq!(started["status"], "failed");
    assert!(started["error"]
        .as_str()
        .unwrap()
        .contains("provider adapter"));

    let listed = app
        .oneshot(
            Request::get("/api/admin/v1/feishu/sync-runs")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    assert_eq!(listed["items"][0]["status"], "failed");
}

#[tokio::test]
async fn sync_and_background_operation_replays_preserve_ids_and_location() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;

    let sync_body = json!({"dryRun": true, "mappingVersion": "idempotency-v1"}).to_string();
    let sync = |body: String| {
        Request::post("/api/admin/v1/feishu/sync-runs")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "sync-replay-contract-0001")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    };
    let first_sync = app.clone().oneshot(sync(sync_body.clone())).await.unwrap();
    assert_eq!(first_sync.status(), StatusCode::ACCEPTED);
    let first_sync_body = response_json(first_sync).await;
    let replay_sync = app.clone().oneshot(sync(sync_body.clone())).await.unwrap();
    assert_eq!(replay_sync.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(replay_sync).await, first_sync_body);
    let different_sync = app
        .clone()
        .oneshot(sync(sync_body.replace("idempotency-v1", "idempotency-v2")))
        .await
        .unwrap();
    assert_eq!(different_sync.status(), StatusCode::CONFLICT);

    let operation_body = json!({
        "kind": "searchReindex",
        "reason": "Rebuild the validated public search projection",
        "confirmation": "REBUILD SEARCH INDEX"
    })
    .to_string();
    let operation = |body: String| {
        Request::post("/api/admin/v1/operations")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "operation-replay-contract-0001")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    };
    let first_operation = app
        .clone()
        .oneshot(operation(operation_body.clone()))
        .await
        .unwrap();
    assert_eq!(first_operation.status(), StatusCode::ACCEPTED);
    let first_location = first_operation
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .clone();
    let first_operation_body = response_json(first_operation).await;
    let replay_operation = app
        .clone()
        .oneshot(operation(operation_body.clone()))
        .await
        .unwrap();
    assert_eq!(replay_operation.status(), StatusCode::ACCEPTED);
    assert_eq!(
        replay_operation.headers().get(header::LOCATION).unwrap(),
        &first_location
    );
    assert_eq!(response_json(replay_operation).await, first_operation_body);
    let different_operation = app
        .clone()
        .oneshot(operation(operation_body.replace(
            "Rebuild the validated public search projection",
            "Use a different validated background task reason",
        )))
        .await
        .unwrap();
    assert_eq!(different_operation.status(), StatusCode::CONFLICT);

    let data = state.data.read().await;
    assert_eq!(data.sync_runs.len(), 1);
    assert_eq!(data.operations.len(), 1);
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "feishu.sync.queue")
            .count(),
        1
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "operation.queue")
            .count(),
        1
    );
}

#[tokio::test]
async fn in_memory_analytics_summary_uses_first_party_counts() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let summary = app
        .oneshot(
            Request::get("/api/admin/v1/analytics/summary")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(summary.status(), StatusCode::OK);
    let summary = response_json(summary).await;
    assert_eq!(summary["acceptedEventCount"], 0);
    assert_eq!(summary["rfqCount"], 0);
    assert_eq!(summary["contactCount"], 0);
    assert_eq!(summary["containsPii"], false);
    assert_eq!(summary["source"], "firstParty");
}

#[tokio::test]
async fn rfq_lists_redact_pii_without_the_dedicated_permission() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "pii-redaction-contract")
                .body(Body::from(
                    json!({
                        "journey": "selection",
                        "contact": {
                            "name": "Protected Buyer",
                            "email": "protected@example.com",
                            "phone": "+1 555 0100",
                            "company": "Example Industry"
                        },
                        "sourcePath": "/en/request-a-quote/selection",
                        "locale": "en",
                        "consent": true,
                        "context": {
                            "application": "Industrial cooling",
                            "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    for user in state.data.write().await.admin_users.values_mut() {
        user.permissions
            .retain(|permission| permission != "rfq.read_pii");
    }
    let listed = app
        .oneshot(
            Request::get("/api/admin/v1/rfqs")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    assert_eq!(
        listed["items"][0]["request"]["contact"]["name"],
        "[restricted]"
    );
    assert_eq!(
        listed["items"][0]["request"]["contact"]["email"],
        "[restricted]"
    );
    assert!(listed["items"][0]["request"]["contact"]["phone"].is_null());
    let stored = state.data.read().await;
    let stored = stored.rfqs.values().next().unwrap();
    assert_eq!(stored.request.contact.name, "Protected Buyer");
    assert_eq!(stored.request.contact.email, "protected@example.com");
}

#[tokio::test]
async fn totp_recovery_codes_and_session_revocation_are_end_to_end_enforced() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let setup_session = setup_admin_without_totp(&app).await;

    let enrollment = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/totp/enrollment")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", &setup_session.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(enrollment.status(), StatusCode::OK);
    assert_eq!(
        enrollment.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );
    let enrollment = response_json(enrollment).await;
    assert_eq!(enrollment["algorithm"], "SHA1");
    assert_eq!(enrollment["digits"], 6);
    let totp = totp_rs::TOTP::from_url(enrollment["otpAuthUri"].as_str().unwrap())
        .expect("valid provisioning URI");
    let current_code = totp.generate_current().expect("system clock");

    let confirmed = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/totp/confirm")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", &setup_session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"code": current_code}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(confirmed.status(), StatusCode::OK);
    assert_eq!(
        confirmed.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );
    let confirmed = response_json(confirmed).await;
    let recovery_code = confirmed["recoveryCodes"][0].as_str().unwrap().to_owned();
    assert_eq!(confirmed["recoveryCodes"].as_array().unwrap().len(), 10);

    let refreshed = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::COOKIE, &setup_session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);
    let refreshed_csrf = refreshed
        .headers()
        .get("x-csrf-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let refreshed_body = response_json(refreshed).await;
    assert_eq!(refreshed_body["totpEnabled"], true);

    let high_risk = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/operations")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", &refreshed_csrf)
                .header("x-totp-code", totp.generate_current().unwrap())
                .header("idempotency-key", "totp-high-risk-contract")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "kind": "backup",
                        "reason": "Verify a real TOTP reauthentication contract",
                        "confirmation": "CREATE BACKUP"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(high_risk.status(), StatusCode::ACCEPTED);

    let logged_out = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::COOKIE, &setup_session.cookie)
                .header("x-csrf-token", refreshed_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);

    let login = |otp: String| {
        Request::post("/api/admin/v1/auth/login")
            .header(header::ORIGIN, "http://localhost:3100")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "email": "admin@example.com",
                    "password": "correct-horse-123",
                    "otp": otp
                })
                .to_string(),
            ))
            .unwrap()
    };

    let totp_login = app
        .clone()
        .oneshot(login(totp.generate_current().unwrap()))
        .await
        .unwrap();
    assert_eq!(totp_login.status(), StatusCode::OK);
    let totp_session = session_credentials(&totp_login);

    let recovery_login = app
        .clone()
        .oneshot(login(recovery_code.clone()))
        .await
        .unwrap();
    assert_eq!(recovery_login.status(), StatusCode::OK);
    let recovery_session = session_credentials(&recovery_login);

    let reused = app.clone().oneshot(login(recovery_code)).await.unwrap();
    assert_eq!(reused.status(), StatusCode::UNAUTHORIZED);

    let sessions = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/sessions")
                .header(header::COOKIE, &recovery_session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sessions.status(), StatusCode::OK);
    let sessions = response_json(sessions).await;
    let other_id = sessions
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["current"] == false)
        .and_then(|session| session["id"].as_str())
        .expect("another active session")
        .to_owned();

    let revoked = app
        .clone()
        .oneshot(
            Request::delete(format!("/api/admin/v1/auth/sessions/{other_id}"))
                .header(header::COOKIE, &recovery_session.cookie)
                .header("x-csrf-token", &recovery_session.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let revoked_session = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::COOKIE, totp_session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked_session.status(), StatusCode::UNAUTHORIZED);

    let audit = state.data.read().await.audit_events.clone();
    assert!(audit
        .iter()
        .any(|event| event.action == "auth.totp.enabled"));
    assert!(audit
        .iter()
        .any(|event| event.action == "auth.session.revoked"));
    assert!(audit.iter().all(|event| {
        let serialized = serde_json::to_string(event).unwrap();
        !serialized.contains(enrollment["secret"].as_str().unwrap_or_default())
            && !serialized.contains(confirmed["recoveryCodes"][0].as_str().unwrap_or_default())
    }));
}
