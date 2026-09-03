#[tokio::test]
async fn product_publish_and_temporary_override_replays_preserve_one_side_effect() {
    let state = AppState::for_test();
    let mut product = published_test_product(
        "idempotent-product",
        "idempotent-product",
        ProductFamily::Axial,
    );
    product.status = PublicationStatus::Draft;
    product.published_revision = None;
    let product_id = product.id;
    insert_publishable_product(&state, product).await;
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;

    let publish = |if_match: &'static str| {
        Request::post(format!("/api/admin/v1/products/{product_id}/publish"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "product-publish-replay-0001")
            .header(header::IF_MATCH, if_match)
            .body(Body::empty())
            .unwrap()
    };
    let first = app
        .clone()
        .oneshot(publish("\"revision-1\""))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(first.headers().get(header::ETAG).unwrap(), "\"revision-1\"");
    let first_body = response_json(first).await;
    let replay = app
        .clone()
        .oneshot(publish("\"revision-1\""))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(
        replay.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(response_json(replay).await, first_body);
    let different = app
        .clone()
        .oneshot(publish("\"revision-2\""))
        .await
        .unwrap();
    assert_eq!(different.status(), StatusCode::CONFLICT);

    let override_body = json!({
        "productId": product_id,
        "fieldPath": "specifications.voltage",
        "value": "230 V",
        "reason": "Temporary verified engineering correction",
        "expiresAt": chrono::Utc::now() + chrono::Duration::days(1)
    })
    .to_string();
    let create_override = |body: String| {
        Request::post(format!(
            "/api/admin/v1/products/{product_id}/temporary-overrides"
        ))
        .header(header::COOKIE, &session.cookie)
        .header("x-csrf-token", &session.csrf)
        .header("idempotency-key", "product-override-replay-0001")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
    };
    let first_override = app
        .clone()
        .oneshot(create_override(override_body.clone()))
        .await
        .unwrap();
    assert_eq!(first_override.status(), StatusCode::CREATED);
    let first_override_body = response_json(first_override).await;
    let replay_override = app
        .clone()
        .oneshot(create_override(override_body.clone()))
        .await
        .unwrap();
    assert_eq!(replay_override.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay_override).await, first_override_body);
    let different_override = app
        .clone()
        .oneshot(create_override(override_body.replace(
            "Temporary verified engineering correction",
            "Different temporary engineering correction",
        )))
        .await
        .unwrap();
    assert_eq!(different_override.status(), StatusCode::CONFLICT);

    let data = state.data.read().await;
    assert_eq!(data.published_products.len(), 1);
    assert_eq!(data.temporary_overrides.len(), 1);
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "product.publish")
            .count(),
        1
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "product.override.create")
            .count(),
        1
    );
}

#[tokio::test]
async fn product_publish_gate_rejects_invalid_master_and_unaccepted_workflow_state() {
    let state = AppState::for_test();
    let mut invalid = published_test_product(
        "publish-gate-invalid-master",
        "publish-gate-invalid-master",
        ProductFamily::Axial,
    );
    invalid.status = PublicationStatus::Draft;
    invalid.published_revision = None;
    invalid.specifications = vec![SpecValue {
        key: "inputPower".into(),
        label: "Input power".into(),
        value: Some(json!(100)),
        unit: Some("watts-ish".into()),
        operating_condition: None,
        state: FactState::PendingVerification,
        source_reference: Some("feishu:test:input-power".into()),
    }];
    invalid.performance_curves = vec![PerformanceCurve {
        airflow_unit: "litres/minute".into(),
        pressure_unit: "Pa".into(),
        speed_rpm: Some(0),
        density_kg_m3: Some(1.2),
        voltage: None,
        test_method: None,
        source_reference: "feishu:test:curve".into(),
        state: FactState::Verified,
        points: vec![airtek_platform::models::CurvePoint {
            airflow: 0.0,
            pressure: 1.0,
        }],
    }];
    let invalid_id = invalid.id;
    insert_publishable_product(&state, invalid).await;

    let mut blocked = published_test_product(
        "publish-gate-blocked-workflow",
        "publish-gate-blocked-workflow",
        ProductFamily::Centrifugal,
    );
    blocked.status = PublicationStatus::Draft;
    blocked.published_revision = None;
    let blocked_id = blocked.id;
    insert_publishable_product(&state, blocked.clone()).await;
    {
        let mut data = state.data.write().await;
        let staging = data
            .staging_records
            .values_mut()
            .find(|record| record.source_snapshot_id == blocked.source_snapshot_id)
            .unwrap();
        staging.validation_status = StagingValidationStatus::Conflicted;
        let run_id = staging.sync_run_id;
        let now = chrono::Utc::now();
        let conflict = SyncConflict {
            id: uuid::Uuid::new_v4(),
            sync_run_id: run_id,
            product_id: Some(blocked_id),
            source_record_id: blocked.stable_id.clone(),
            diffs: vec![],
            resolved_at: None,
            resolution: None,
        };
        data.conflicts.insert(conflict.id, conflict);
        let expired_override = TemporaryOverride {
            id: uuid::Uuid::new_v4(),
            product_id: blocked_id,
            field_path: "specifications.inputPower".into(),
            value: json!(100),
            reason: "Test-only expired engineering override".into(),
            created_at: now - chrono::Duration::days(2),
            expires_at: now - chrono::Duration::days(1),
            expired: true,
        };
        data.temporary_overrides
            .insert(expired_override.id, expired_override);
    }

    let app = build_router(state);
    let session = setup_admin(&app).await;
    let publish = |id: uuid::Uuid, key: &'static str| {
        Request::post(format!("/api/admin/v1/products/{id}/publish"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", key)
            .header(header::IF_MATCH, "\"revision-1\"")
            .body(Body::empty())
            .unwrap()
    };

    let invalid_response = app
        .clone()
        .oneshot(publish(invalid_id, "invalid-master-gate-0001"))
        .await
        .unwrap();
    assert_eq!(invalid_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let invalid_problem = response_json(invalid_response).await;
    assert!(invalid_problem["errors"]["specifications[0].unit"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty()));
    assert!(invalid_problem["errors"]["performanceCurves[0].points"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty()));

    let blocked_response = app
        .oneshot(publish(blocked_id, "blocked-workflow-gate-0001"))
        .await
        .unwrap();
    assert_eq!(blocked_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let blocked_problem = response_json(blocked_response).await;
    for field in [
        "staging.validationStatus",
        "conflicts",
        "temporaryOverrides",
    ] {
        assert!(
            blocked_problem["errors"][field]
                .as_array()
                .is_some_and(|errors| !errors.is_empty()),
            "{field}"
        );
    }
}

#[tokio::test]
async fn admin_business_mutations_require_session_and_csrf() {
    let app = build_router(AppState::for_test());
    let body = json!({
        "kind": "article",
        "slug": "csrf-contract",
        "locale": "en",
        "title": "CSRF contract",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {"indexable": false},
        "isPlaceholder": true
    })
    .to_string();
    let unauthenticated = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let session = setup_admin(&app).await;
    let missing_csrf = app
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, session.cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn identity_mutations_enforce_and_replay_idempotency_keys() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let user_id = *state
        .data
        .read()
        .await
        .admin_users
        .keys()
        .next()
        .expect("setup administrator");
    let update_body = json!({
        "displayName": "Idempotent Administrator",
        "reason": "Verify identity mutation idempotency"
    })
    .to_string();
    let update_request = |key: Option<&str>| {
        let mut request = Request::patch(format!("/api/admin/v1/users/{user_id}"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::IF_MATCH, "\"revision-1\"")
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(key) = key {
            request = request.header("idempotency-key", key);
        }
        request.body(Body::from(update_body.clone())).unwrap()
    };

    let missing = app.clone().oneshot(update_request(None)).await.unwrap();
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    assert!(response_json(missing).await["detail"]
        .as_str()
        .unwrap()
        .contains("Idempotency-Key"));

    let key = "identity-user-update-replay-0001";
    let first = app
        .clone()
        .oneshot(update_request(Some(key)))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_etag = first.headers().get(header::ETAG).unwrap().clone();
    let first_body = first.into_body().collect().await.unwrap().to_bytes();
    let replay = app
        .clone()
        .oneshot(update_request(Some(key)))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(replay.headers().get(header::ETAG).unwrap(), first_etag);
    assert_eq!(
        replay.into_body().collect().await.unwrap().to_bytes(),
        first_body
    );

    let missing_invitation_key = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/user-invitations")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "email": "invitee@example.com",
                        "displayName": "Invitee",
                        "roleKeys": ["content-editor"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_invitation_key.status(), StatusCode::BAD_REQUEST);

    let missing_revoke_key = app
        .clone()
        .oneshot(
            Request::post(format!(
                "/api/admin/v1/user-invitations/{}/revoke",
                uuid::Uuid::new_v4()
            ))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"reason": "Revoke unused invitation safely"}).to_string(),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_revoke_key.status(), StatusCode::BAD_REQUEST);

    let missing_role_key = app
        .oneshot(
            Request::patch(format!("/api/admin/v1/roles/{}", uuid::Uuid::new_v4()))
                .header(header::COOKIE, session.cookie)
                .header("x-csrf-token", session.csrf)
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "displayName": "Auditable Role",
                        "reason": "Verify role mutation idempotency"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_role_key.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn logout_requires_csrf_for_an_active_session_before_revocation() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let rejected = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let still_active = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(still_active.status(), StatusCode::OK);
    let refreshed_csrf = still_active
        .headers()
        .get("x-csrf-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let logged_out = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", refreshed_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);

    let revoked = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::COOKIE, session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    let idempotent = app
        .oneshot(
            Request::post("/api/admin/v1/auth/logout")
                .header(header::ORIGIN, "http://localhost:3100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(idempotent.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn bootstrap_token_is_not_evaluated_after_initial_setup() {
    let app = build_router(AppState::for_test());
    let _ = setup_admin(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/admin/v1/auth/setup")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "displayName": "Second Administrator",
                        "email": "second@example.com",
                        "password": "another-password-123",
                        "bootstrapToken": "intentionally-wrong-token-value"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}
