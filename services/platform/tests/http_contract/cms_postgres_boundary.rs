#[tokio::test]
async fn unified_cms_refuses_the_in_memory_adapter() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "cms-postgres-only-0001")
                .header(header::IF_MATCH, "\"draft-0\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "schemaVersion": 2,
                        "kind": "navigation",
                        "locale": "en",
                        "templateKey": "navigation",
                        "title": "Primary Navigation",
                        "slug": null,
                        "summary": null,
                        "isPlaceholder": true,
                        "typeFields": {"type": "navigation", "items": []},
                        "body": null,
                        "composition": {"blocks": []},
                        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
                        "relations": [],
                        "draftVersion": 1
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
}
