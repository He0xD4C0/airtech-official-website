#[tokio::test]
async fn public_projection_survives_draft_edits_and_rollback_republishes_history() {
    let state = AppState::for_test();
    install_published_site_shell(&state).await;
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let original = json!({
        "kind": "article",
        "slug": "immutable-projection",
        "locale": "en",
        "title": "Original published title",
        "summary": "Published discovery summary",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": [{"type": "paragraph"}]}},
        "seo": {"indexable": true, "canonicalPath": "/en/resources/articles/immutable-projection"},
        "isPlaceholder": false
    });
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "projection-create-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(original.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let publish_request = || {
        Request::post(format!("/api/admin/v1/content/{id}/publish"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "projection-publish-0001")
            .header(header::IF_MATCH, "\"revision-1\"")
            .body(Body::empty())
            .unwrap()
    };
    let published = app.clone().oneshot(publish_request()).await.unwrap();
    assert_eq!(published.status(), StatusCode::OK);
    assert_eq!(
        published.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    let published_body = response_json(published).await;
    let publish_replay = app.clone().oneshot(publish_request()).await.unwrap();
    assert_eq!(publish_replay.status(), StatusCode::OK);
    assert_eq!(
        publish_replay.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(response_json(publish_replay).await, published_body);

    let mut draft = original.clone();
    draft["title"] = json!("Unpublished replacement title");
    let updated = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::IF_MATCH, "\"revision-1\"")
                .header("idempotency-key", "projection-update-0001")
                .body(Body::from(draft.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let public_before_rollback = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content/articles/immutable-projection")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public_before_rollback.status(), StatusCode::OK);
    assert_eq!(
        public_before_rollback.headers().get(header::ETAG).unwrap(),
        "\"revision-1\""
    );
    assert_eq!(
        response_json(public_before_rollback).await["title"],
        "Original published title"
    );

    let rollback_body =
        json!({"revision": 1, "reason": "Restore validated public copy"}).to_string();
    let rollback_request = || {
        Request::post(format!("/api/admin/v1/content/{id}/rollback"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", "projection-rollback-0001")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, "\"revision-2\"")
            .body(Body::from(rollback_body.clone()))
            .unwrap()
    };
    let rolled_back = app.clone().oneshot(rollback_request()).await.unwrap();
    assert_eq!(rolled_back.status(), StatusCode::OK);
    assert_eq!(
        rolled_back.headers().get(header::ETAG).unwrap(),
        "\"revision-3\""
    );
    let rolled_back_body = response_json(rolled_back).await;
    let rollback_replay = app.clone().oneshot(rollback_request()).await.unwrap();
    assert_eq!(rollback_replay.status(), StatusCode::OK);
    assert_eq!(
        rollback_replay.headers().get(header::ETAG).unwrap(),
        "\"revision-3\""
    );
    assert_eq!(response_json(rollback_replay).await, rolled_back_body);

    let public_after_rollback = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content/articles/immutable-projection")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(public_after_rollback.status(), StatusCode::OK);
    assert_eq!(
        public_after_rollback.headers().get(header::ETAG).unwrap(),
        "\"revision-3\""
    );
    assert_eq!(
        response_json(public_after_rollback).await["title"],
        "Original published title"
    );
    let discovery = app
        .oneshot(
            Request::get("/api/public/v1/discovery")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(discovery.status(), StatusCode::OK);
    let discovery = response_json(discovery).await;
    assert_eq!(discovery["entries"].as_array().unwrap().len(), 1);
    assert_eq!(
        discovery["entries"][0]["path"],
        "/en/resources/articles/immutable-projection"
    );
    assert_eq!(discovery["entries"][0]["title"], "Original published title");
    assert_eq!(
        discovery["entries"][0]["summary"],
        "Published discovery summary"
    );
    let data = state.data.read().await;
    assert_eq!(data.outbox_events.len(), 2);
    assert_eq!(
        data.content_revisions[&uuid::Uuid::parse_str(&id).unwrap()].len(),
        2
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "content.publish")
            .count(),
        1
    );
    assert_eq!(
        data.audit_events
            .iter()
            .filter(|event| event.action == "content.rollback")
            .count(),
        1
    );
}

#[tokio::test]
async fn generic_content_endpoints_exclude_news_and_fail_closed_for_news_mutations() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let content = json!({
        "kind": "news",
        "slug": "dedicated-news-boundary",
        "locale": "en",
        "title": "Dedicated News boundary",
        "summary": "News metadata must remain transactionally consistent.",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {
            "indexable": true,
            "canonicalPath": "/en/resources/news/dedicated-news-boundary"
        },
        "isPlaceholder": false
    });

    let generic_create = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "generic-news-rejected-create")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(content.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generic_create.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(response_json(generic_create).await["errors"]["kind"][0]
        .as_str()
        .unwrap()
        .contains("/api/admin/v1/news"));

    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/news")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "dedicated-news-create-boundary")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "content": content,
                        "category": "Company",
                        "authorDisplayName": "AIRTEKPOWER",
                        "coverMediaId": null,
                        "publishedAt": null,
                        "featured": false,
                        "dataClass": "editorial"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let id = created["content"]["id"].as_str().unwrap();

    let generic_list = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/content?limit=100")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(generic_list.status(), StatusCode::OK);
    assert!(response_json(generic_list).await["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["kind"] != "news"));

    let mut generic_update = created["content"].clone();
    generic_update["kind"] = json!("article");
    generic_update.as_object_mut().unwrap().retain(|key, _| {
        matches!(
            key.as_str(),
            "kind" | "slug" | "locale" | "title" | "summary" | "body" | "seo" | "isPlaceholder"
        )
    });
    let update = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "generic-news-rejected-update")
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(generic_update.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::CONFLICT);
    assert!(response_json(update).await["detail"]
        .as_str()
        .unwrap()
        .contains("/api/admin/v1/news"));

    for (path, body, key) in [
        (
            format!("/api/admin/v1/content/{id}/publish"),
            None,
            "generic-news-rejected-publish",
        ),
        (
            format!("/api/admin/v1/content/{id}/rollback"),
            Some(json!({"revision": 1, "reason": "Use the dedicated News workflow"})),
            "generic-news-rejected-rollback",
        ),
    ] {
        let has_body = body.is_some();
        let mut request = Request::post(path)
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", key)
            .header(header::IF_MATCH, "\"revision-1\"")
            .body(Body::from(
                body.map_or_else(String::new, |value| value.to_string()),
            ))
            .unwrap();
        if has_body {
            request
                .headers_mut()
                .insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
        }
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(response_json(response).await["detail"]
            .as_str()
            .unwrap()
            .contains("/api/admin/v1/news"));
    }
}
