#[tokio::test]
async fn login_failures_are_rate_limited_by_source_and_account() {
    let app = build_router(AppState::for_test());
    let _ = setup_admin(&app).await;
    let request = || {
        Request::post("/api/admin/v1/auth/login")
            .header(header::ORIGIN, "http://localhost:3100")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-real-ip", "192.0.2.10")
            .body(Body::from(
                json!({
                    "email": "admin@example.com",
                    "password": "definitely-not-correct"
                })
                .to_string(),
            ))
            .unwrap()
    };
    for _ in 0..5 {
        let response = app.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let blocked = app.oneshot(request()).await.unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn high_risk_operations_fail_closed_until_totp_is_enabled() {
    let app = build_router(AppState::for_test());
    let session = setup_admin_without_totp(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/admin/v1/operations")
                .header(header::COOKIE, session.cookie)
                .header("x-csrf-token", session.csrf)
                .header("x-totp-code", "123456")
                .header("idempotency-key", "operation-fail-closed-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "kind": "backup",
                        "reason": "Create verified backup before deployment",
                        "confirmation": "CREATE BACKUP"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response_json(response).await["detail"]
        .as_str()
        .unwrap()
        .contains("enable TOTP"));
}

#[tokio::test]
async fn application_operations_cannot_apply_flyway_migrations() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/admin/v1/operations")
                .header(header::COOKIE, session.cookie)
                .header("x-csrf-token", session.csrf)
                .header("idempotency-key", "flyway-deployment-only-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "kind": "migrationApply",
                        "reason": "Attempt migration through the application boundary",
                        "confirmation": "APPLY MIGRATION"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(response_json(response).await["detail"]
        .as_str()
        .unwrap()
        .contains("Flyway"));
}
