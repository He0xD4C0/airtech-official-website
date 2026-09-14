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
async fn general_operations_collection_is_removed() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    for method in [axum::http::Method::GET, axum::http::Method::POST] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/api/admin/v1/operations")
                    .header(header::COOKIE, &session.cookie)
                    .header("x-csrf-token", &session.csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
