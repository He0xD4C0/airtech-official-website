#[tokio::test]
async fn disabled_feishu_and_removed_operations_create_no_records() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;

    let sync = app.clone().oneshot(
        Request::post("/api/admin/v1/feishu/sync-runs")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"dryRun": true, "mappingVersion": "v1"}).to_string()))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(sync.status(), StatusCode::CONFLICT);

    let operation = app.clone().oneshot(
        Request::post("/api/admin/v1/operations")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(operation.status(), StatusCode::NOT_FOUND);

    let summary = app.oneshot(
        Request::get("/api/admin/v1/analytics/summary")
            .header(header::COOKIE, session.cookie)
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(summary.status(), StatusCode::NOT_FOUND);

    let data = state.data.read().await;
    assert!(data.sync_runs.is_empty());
    assert!(data.operations.is_empty());
    assert!(data.audit_events.iter().all(|event| {
        event.action != "feishu.sync.queue" && event.action != "operation.queue"
    }));
}

#[tokio::test]
async fn operation_status_exposes_only_product_imports() {
    let state = AppState::for_test();
    let now = Utc::now();
    let product_import = BackgroundOperation {
        id: Uuid::new_v4(),
        kind: OperationKind::ProductImport,
        status: OperationStatus::Completed,
        reason: "Product Master import".into(),
        created_at: now,
        updated_at: now,
        result: Some(json!({"import": {}})),
    };
    let retired = BackgroundOperation {
        id: Uuid::new_v4(),
        kind: OperationKind::Backup,
        status: OperationStatus::Failed,
        reason: "Historical backup operation".into(),
        created_at: now,
        updated_at: now,
        result: None,
    };
    {
        let mut data = state.data.write().await;
        data.operations.insert(product_import.id, product_import.clone());
        data.operations.insert(retired.id, retired.clone());
    }
    let app = build_router(state);
    let session = setup_admin(&app).await;
    let visible = app.clone().oneshot(
        Request::get(format!("/api/admin/v1/operations/{}", product_import.id))
            .header(header::COOKIE, &session.cookie)
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(visible.status(), StatusCode::OK);
    let hidden = app.oneshot(
        Request::get(format!("/api/admin/v1/operations/{}", retired.id))
            .header(header::COOKIE, session.cookie)
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rfq_lists_are_always_redacted_and_keep_pii_out_of_the_wire_shape() {
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
    assert_eq!(listed["items"][0]["entityType"], "rfq");
    assert_eq!(listed["items"][0]["organization"], "Example Industry");
    assert!(listed["items"][0].get("request").is_none());
    assert!(listed["items"][0].get("name").is_none());
    assert!(listed["items"][0].get("email").is_none());
    assert!(listed["items"][0].get("phone").is_none());
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
