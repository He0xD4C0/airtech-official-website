#[tokio::test]
async fn signed_content_preview_is_exact_short_lived_private_and_audited() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let original = json!({
        "kind": "article",
        "slug": "private-preview",
        "locale": "en",
        "title": "First private draft",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "First revision"}]}]}},
        "seo": {"indexable": true, "canonicalPath": "/en/resources/articles/private-preview"},
        "isPlaceholder": false
    });
    let created = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header("idempotency-key", "preview-content-create-0001")
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

    let mut second = original.clone();
    second["title"] = json!("Second private draft");
    let updated = app
        .clone()
        .oneshot(
            Request::patch(format!("/api/admin/v1/content/{id}"))
                .header(header::COOKIE, &session.cookie)
                .header("x-csrf-token", &session.csrf)
                .header(header::IF_MATCH, "\"revision-1\"")
                .header("idempotency-key", "preview-update-0001")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(second.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let issue = |revision: i64, if_match: Option<&str>, expires_in_seconds: u32| {
        let mut request = Request::post(format!("/api/admin/v1/content/{id}/preview"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "revision": revision,
                    "expiresInSeconds": expires_in_seconds
                })
                .to_string(),
            ))
            .unwrap();
        if let Some(if_match) = if_match {
            request
                .headers_mut()
                .insert(header::IF_MATCH, if_match.parse().unwrap());
        }
        request
    };

    let stale = app
        .clone()
        .oneshot(issue(2, Some("\"revision-1\""), 600))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let missing_revision = app.clone().oneshot(issue(99, None, 600)).await.unwrap();
    assert_eq!(missing_revision.status(), StatusCode::NOT_FOUND);

    let issued = app
        .clone()
        .oneshot(issue(2, Some("\"revision-2\""), 600))
        .await
        .unwrap();
    assert_eq!(issued.status(), StatusCode::CREATED);
    assert_eq!(
        issued.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let location = issued
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(location.starts_with("http://localhost:3000/en/preview?token=v1."));
    let issued_body = response_json(issued).await;
    assert_eq!(issued_body["url"], location);
    assert_eq!(issued_body["revision"], 2);
    let token = location.split_once("?token=").unwrap().1.to_owned();

    let preview = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    assert_eq!(
        preview.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    assert_eq!(
        preview.headers().get("x-robots-tag").unwrap(),
        "noindex, nofollow, noarchive"
    );
    let preview_body = response_json(preview).await;
    assert_eq!(preview_body["content"]["title"], "Second private draft");
    assert_eq!(preview_body["content"]["currentRevision"], 2);
    assert_eq!(preview_body["content"]["status"], "draft");

    let unavailable_published_fallback = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content/articles/private-preview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        unavailable_published_fallback.status(),
        StatusCode::NOT_FOUND
    );

    let unauthenticated = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        unauthenticated
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap(),
        "private, no-store, max-age=0"
    );

    let mut tampered = token.into_bytes();
    let index = tampered.len() / 2;
    tampered[index] = if tampered[index] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(tampered).unwrap();
    let rejected = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {tampered}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::NOT_FOUND);

    let first_revision = app.clone().oneshot(issue(1, None, 600)).await.unwrap();
    let first_location = first_revision
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let first_token = first_location.split_once("?token=").unwrap().1;
    let exact_first = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(exact_first.status(), StatusCode::OK);
    assert_eq!(
        response_json(exact_first).await["content"]["title"],
        "First private draft"
    );

    state
        .data
        .write()
        .await
        .admin_sessions
        .values_mut()
        .next()
        .expect("issuing Admin session")
        .revoked = true;
    let revoked_session_preview = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {first_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked_session_preview.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        revoked_session_preview
            .headers()
            .get(header::CACHE_CONTROL)
            .unwrap(),
        "private, no-store, max-age=0"
    );

    let audit = state.data.read().await.audit_events.clone();
    let preview_audits: Vec<_> = audit
        .iter()
        .filter(|event| event.action == "content.preview.issue")
        .collect();
    assert_eq!(preview_audits.len(), 2);
    let serialized_audit = serde_json::to_string(&preview_audits).unwrap();
    assert!(!serialized_audit.contains("?token="));
    assert!(!serialized_audit.contains("v1."));
}

#[tokio::test]
async fn expired_content_preview_returns_gone_without_fallback() {
    let state = AppState::for_test();
    let content_id = uuid::Uuid::new_v4();
    let content = airtek_platform::models::ContentEntry {
        id: content_id,
        kind: airtek_platform::models::ContentKind::Article,
        slug: "expiring-preview".into(),
        locale: "en".into(),
        title: "Expiring preview".into(),
        summary: None,
        body: airtek_platform::models::RichTextDocument {
            schema_version: 1,
            doc: json!({"type": "doc", "content": []}),
        },
        seo: airtek_platform::models::SeoMetadata::default(),
        status: PublicationStatus::Draft,
        is_placeholder: false,
        current_revision: 1,
        published_revision: None,
        scheduled_for: None,
        updated_at: chrono::Utc::now(),
    };
    state
        .data
        .write()
        .await
        .content_revisions
        .entry(content_id)
        .or_default()
        .insert(1, content);
    let key = state.config.preview_signing_key.as_ref().unwrap();
    let token = airtek_platform::preview_token::issue(
        key,
        content_id,
        1,
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        1,
    )
    .unwrap()
    .token;
    tokio::time::sleep(std::time::Duration::from_millis(1_100)).await;

    let response = build_router(state)
        .oneshot(
            Request::get("/api/public/v1/content-preview")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::GONE);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
}

#[tokio::test]
async fn rich_text_entity_block_kind_is_allowlisted() {
    let app = build_router(AppState::for_test());
    let session = setup_admin(&app).await;
    let request = |kind: &str| {
        Request::post("/api/admin/v1/content")
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", format!("rich-text-{kind}-0001"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "kind": "article",
                    "slug": format!("entity-{kind}"),
                    "locale": "en",
                    "title": "Entity block",
                    "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": [{"type": "entityBlock", "attrs": {"kind": kind}}]}},
                    "seo": {"indexable": false},
                    "isPlaceholder": true
                })
                .to_string(),
            ))
            .unwrap()
    };
    let allowed = app
        .clone()
        .oneshot(request("related-product"))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::CREATED);
    let rejected = app.oneshot(request("raw-html")).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
