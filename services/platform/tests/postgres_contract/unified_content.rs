#[cfg(feature = "devtools")]
fn cms_principal() -> AdminPrincipal {
    AdminPrincipal {
        user_id: Uuid::new_v4(),
        display_name: "PostgreSQL CMS Tester".into(),
        email: "postgres-cms@example.com".into(),
        role: "super-admin".into(),
        permissions: vec![
            "content.read".into(),
            "content.write".into(),
            "content.publish".into(),
        ],
        session_id: Uuid::new_v4(),
        session_token_hash: vec![1; 32],
        csrf_hash: vec![2; 32],
        totp_enabled: true,
    }
}

#[cfg(feature = "devtools")]
fn cms_document(kind: &str, suffix: &str) -> Value {
    let identity = Uuid::new_v4();
    let shared = json!({
        "schemaVersion": 2,
        "kind": kind,
        "locale": "en",
        "title": format!("{kind} {suffix}"),
        "summary": null,
        "isPlaceholder": true,
        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
        "relations": [],
        "draftVersion": 1
    });
    match kind {
        "news" => merge_json(shared, json!({
            "slug": format!("news-{suffix}"),
            "templateKey": "newsDetail",
            "typeFields": {
                "type": "news", "category": "Company", "authorDisplayName": "AIRTEKPOWER",
                "publicationAt": null, "cover": null, "featured": false
            },
            "body": {"type": "doc", "content": [{"type": "paragraph"}]},
            "composition": {"blocks": [
                {"type": "hero", "id": identity, "eyebrow": null, "heading": "News",
                 "lead": null, "media": null, "actions": [], "variant": "standard"},
                {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
            ]}
        })),
        "generalInformation" => merge_json(shared, json!({
            "slug": null,
            "templateKey": "generalInformation",
            "typeFields": {
                "type": "generalInformation", "organizationName": "AIRTEKPOWER",
                "brandLine": null, "homePath": "/en", "footerStatement": null,
                "copyrightTemplate": null,
                "contact": {"email": null, "phone": null, "addressLines": [],
                    "locality": null, "region": null, "postalCode": null, "countryCode": null},
                "socialLinks": [],
                "defaultSeo": {"title": null, "description": null, "indexable": false,
                    "socialImage": null},
                "productCategories": [], "navigationCta": null
            },
            "body": null, "composition": {"blocks": []}
        })),
        "navigation" => merge_json(shared, json!({
            "slug": null, "templateKey": "navigation",
            "typeFields": {"type": "navigation", "items": []},
            "body": null, "composition": {"blocks": []}
        })),
        "footer" => merge_json(shared, json!({
            "slug": null, "templateKey": "footer",
            "typeFields": {"type": "footer", "columns": [], "legalLinks": []},
            "body": null, "composition": {"blocks": []}
        })),
        _ => panic!("unsupported test kind"),
    }
}

#[cfg(feature = "devtools")]
fn merge_json(mut base: Value, extra: Value) -> Value {
    base.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    base
}

#[cfg(feature = "devtools")]
async fn cms_mutation(
    app: &Router,
    method: Method,
    path: &str,
    version: i64,
    body: Value,
) -> Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::IF_MATCH, format!("\"draft-{version}\""))
                .header("idempotency-key", format!("cms-contract-{}", Uuid::new_v4()))
                .header("x-actor", "postgres-cms@example.com")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn unified_cms_round_trips_all_configuration_content_and_revisions() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 11).await;
    let pool = sandbox.pool();
    let state = postgres_state(sandbox.connection_url());
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    for kind in ["news", "generalInformation", "navigation", "footer"] {
        let suffix = Uuid::new_v4().simple().to_string();
        let created = cms_mutation(&app, Method::POST, "/content", 0, cms_document(kind, &suffix)).await;
        assert_eq!(created.status(), StatusCode::CREATED, "create {kind}");
        assert_eq!(created.headers()[header::ETAG], "\"draft-1\"");
        let created = response_json(created).await;
        let id = created["id"].as_str().unwrap();
        assert_eq!(created["draft"]["kind"], kind);

        let mut saved_document = created["draft"].clone();
        saved_document["title"] = json!(format!("Saved {kind}"));
        let saved = cms_mutation(
            &app,
            Method::PATCH,
            &format!("/content/{id}/draft"),
            1,
            saved_document.clone(),
        )
        .await;
        assert_eq!(saved.status(), StatusCode::OK, "save {kind}");
        let saved = response_json(saved).await;
        assert_eq!(saved["draft"]["draftVersion"], 2);
        let reloaded = app
            .clone()
            .oneshot(
                Request::get(format!("/content/{id}/draft"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reloaded.status(), StatusCode::OK, "reload {kind}");
        assert_eq!(reloaded.headers()[header::ETAG], "\"draft-2\"");
        assert_eq!(
            response_json(reloaded).await["draft"]["title"],
            format!("Saved {kind}")
        );
        let revision_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM content_revisions WHERE content_id=$1 AND document IS NOT NULL",
        )
        .bind(Uuid::parse_str(id).unwrap())
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(revision_count, 0, "autosave must not create revision");

        let stale = cms_mutation(
            &app,
            Method::PATCH,
            &format!("/content/{id}/draft"),
            1,
            saved_document,
        )
        .await;
        assert_eq!(stale.status(), StatusCode::CONFLICT, "stale {kind}");

        let manual = cms_mutation(
            &app,
            Method::POST,
            &format!("/content/{id}/snapshots"),
            2,
            json!({"intent": "manual", "reason": "Record reviewed manual snapshot"}),
        )
        .await;
        assert_eq!(manual.status(), StatusCode::CREATED, "manual {kind}");
        assert_eq!(response_json(manual).await["latestRevision"], 1);

        let mut publish_document = saved["draft"].clone();
        publish_document["title"] = json!(format!("Published {kind}"));
        let publish_draft = cms_mutation(
            &app,
            Method::PATCH,
            &format!("/content/{id}/draft"),
            2,
            publish_document,
        )
        .await;
        assert_eq!(publish_draft.status(), StatusCode::OK);
        let publish_draft = response_json(publish_draft).await;
        assert_eq!(publish_draft["draft"]["draftVersion"], 3);

        let published = cms_mutation(
            &app,
            Method::POST,
            &format!("/content/{id}/snapshots"),
            3,
            json!({"intent": "publish", "reason": "Publish reviewed CMS content"}),
        )
        .await;
        assert_eq!(published.status(), StatusCode::CREATED, "publish {kind}");
        let published = response_json(published).await;
        assert_eq!(published["publishedRevision"], 2);

        let diff = app
            .clone()
            .oneshot(
                Request::get(format!("/content/{id}/diff?baseRevision=1&targetRevision=2"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(diff.status(), StatusCode::OK);
        assert!(response_json(diff).await["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["path"] == "/title"));

        let restored = cms_mutation(
            &app,
            Method::POST,
            &format!("/content/{id}/revisions/1/restore"),
            3,
            json!({"reason": "Restore the reviewed manual snapshot"}),
        )
        .await;
        assert_eq!(restored.status(), StatusCode::CREATED, "restore {kind}");
        let restored = response_json(restored).await;
        assert_eq!(restored["draft"]["title"], format!("Saved {kind}"));
        assert_eq!(restored["draft"]["draftVersion"], 4);
        assert_eq!(restored["latestRevision"], 3);
        assert_eq!(restored["publishedRevision"], 2);

        let revisions = app
            .clone()
            .oneshot(
                Request::get(format!("/content/{id}/revisions?limit=100"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(revisions.status(), StatusCode::OK);
        let revisions = response_json(revisions).await;
        assert_eq!(revisions["items"].as_array().unwrap().len(), 3);
        assert_eq!(revisions["items"][0]["kind"], "restore");

        if kind == "news" {
            assert_eq!(restored["draft"]["typeFields"]["category"], "Company");
        }
        if kind == "generalInformation" {
            assert_eq!(
                restored["draft"]["typeFields"]["organizationName"],
                "AIRTEKPOWER"
            );
        }
    }

    for path in ["/news", "/general-information"] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "legacy {path}");
    }

    sandbox.cleanup().await;
}

#[cfg(feature = "devtools")]
async fn admin_get_json(app: &Router, path: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    (status, response_json(response).await)
}

#[cfg(feature = "devtools")]
fn item_ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|item| item["id"].as_str().expect("item id").to_owned())
        .collect()
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn unified_cms_list_filters_sort_and_counts_cover_more_than_one_page() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 11).await;
    let state = postgres_state(sandbox.connection_url());
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    for index in 0..100 {
        let suffix = format!("{index:03}");
        let created = cms_mutation(
            &app,
            Method::POST,
            "/content",
            0,
            cms_document("news", &suffix),
        )
        .await;
        assert_eq!(created.status(), StatusCode::CREATED, "create news {suffix}");
    }
    // Navigation, Footer, and General Information stay singleton per locale.
    for kind in ["navigation", "footer", "generalInformation"] {
        let created = cms_mutation(
            &app,
            Method::POST,
            "/content",
            0,
            cms_document(kind, "singleton"),
        )
        .await;
        assert_eq!(created.status(), StatusCode::CREATED, "create {kind}");
    }

    // Page one returns 50 of 103 records while counts describe the whole set.
    let (status, page) = admin_get_json(&app, "/content?limit=50").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"].as_array().unwrap().len(), 50);
    assert_eq!(page["total"], 103);
    assert_eq!(page["counts"]["news"], 100);
    assert_eq!(page["counts"]["navigation"], 1);
    assert_eq!(page["counts"]["footer"], 1);
    assert_eq!(page["counts"]["generalInformation"], 1);
    let next_cursor = page["nextCursor"]
        .as_str()
        .expect("first page exposes a cursor")
        .to_owned();
    let first_page_ids = item_ids(&page);

    // A kind filter narrows items without shrinking the type counts.
    let (status, filtered) = admin_get_json(&app, "/content?kind=news&limit=50").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(filtered["items"].as_array().unwrap().len(), 50);
    assert_eq!(filtered["total"], 103);
    assert_eq!(filtered["counts"]["news"], 100);
    assert!(filtered["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["draft"]["kind"] == "news"));

    // The cursor continues the same selection without repeating records.
    let (status, second) = admin_get_json(
        &app,
        &format!("/content?limit=50&cursor={next_cursor}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_page_ids = item_ids(&second);
    assert!(!second_page_ids.is_empty());
    assert!(second_page_ids
        .iter()
        .all(|id| !first_page_ids.contains(id)));

    // Cursors are bound to the filter signature and cannot be replayed.
    let (status, _) = admin_get_json(
        &app,
        &format!("/content?kind=footer&limit=50&cursor={next_cursor}"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Search matches title or slug on the server.
    let (status, by_title) = admin_get_json(&app, "/content?q=news%20042").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_title["items"].as_array().unwrap().len(), 1);
    assert_eq!(by_title["items"][0]["draft"]["title"], "news 042");
    let (status, by_slug) = admin_get_json(&app, "/content?q=news-042").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_slug["items"].as_array().unwrap().len(), 1);

    // Status filtering and title sorting are server-side.
    let (status, drafts) =
        admin_get_json(&app, "/content?status=draft&sort=title&direction=asc&limit=100").await;
    assert_eq!(status, StatusCode::OK);
    let titles: Vec<String> = drafts["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            item["draft"]["title"]
                .as_str()
                .expect("draft title")
                .to_lowercase()
        })
        .collect();
    let mut sorted = titles.clone();
    sorted.sort();
    assert_eq!(titles, sorted);

    // Invalid filters fail closed instead of being ignored.
    for path in [
        "/content?status=scheduled",
        "/content?sort=publishedAt",
        "/content?direction=sideways",
        "/content?kind=productDetail",
    ] {
        let (status, _) = admin_get_json(&app, path).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "invalid filter {path}");
    }

    // The controlled template registry is exposed for the editor.
    let (status, templates) = admin_get_json(&app, "/content/templates").await;
    assert_eq!(status, StatusCode::OK);
    let items = templates["items"].as_array().expect("template items");
    assert_eq!(items.len(), 28);
    let find = |key: &str| {
        items
            .iter()
            .find(|item| item["key"] == key)
            .unwrap_or_else(|| panic!("template {key}"))
    };
    let navigation = find("navigation");
    assert_eq!(navigation["contentKind"], "navigation");
    assert_eq!(navigation["routable"], false);
    assert_eq!(navigation["singletonPerLocale"], true);
    assert_eq!(navigation["allowedBlocks"].as_array().unwrap().len(), 0);
    assert_eq!(
        find("solutionDetail")["requiredBlocks"],
        json!(["hero", "body"])
    );

    sandbox.cleanup().await;
}
