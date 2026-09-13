#[cfg(feature = "devtools")]
fn projection_news_document(slug: &str, indexable: bool) -> Value {
    json!({
        "schemaVersion": 2,
        "kind": "news",
        "locale": "en",
        "templateKey": "newsDetail",
        "title": format!("Projection {slug}"),
        "slug": slug,
        "summary": "Published through unified CMS",
        "isPlaceholder": false,
        "seo": {
            "title": "Projection title",
            "description": "Projection description",
            "indexable": indexable,
            "socialImage": null
        },
        "typeFields": {
            "type": "news",
            "category": "Company",
            "authorDisplayName": "AIRTEKPOWER",
            "publicationAt": null,
            "cover": null,
            "featured": false
        },
        "body": {"type": "doc", "content": [{"type": "paragraph"}]},
        "composition": {"blocks": [
            {
                "type": "hero",
                "id": Uuid::new_v4(),
                "eyebrow": "News",
                "heading": "Projection heading",
                "lead": null,
                "media": null,
                "actions": [],
                "variant": "standard"
            },
            {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
        ]},
        "relations": [],
        "draftVersion": 1
    })
}

#[cfg(feature = "devtools")]
async fn create_projection_content(app: &Router, document: Value) -> String {
    let created = cms_mutation(app, Method::POST, "/content", 0, document).await;
    assert_eq!(
        created.status(),
        StatusCode::CREATED,
        "create projection content"
    );
    response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[cfg(feature = "devtools")]
async fn public_get(app: &Router, uri: &str) -> Response {
    let mut request = Request::get(uri).body(Body::empty()).unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from((Ipv6Addr::LOCALHOST, 40_000))));
    app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn publishing_cms_v2_content_writes_the_canonical_route_and_serves_the_projection() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let pool = sandbox.pool();
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state.clone());
    let app = airtek_platform::build_router(state);

    for kind in ["generalInformation", "navigation", "footer"] {
        let mut document = cms_document(kind, "public-projection-shell");
        document["isPlaceholder"] = json!(false);
        let shell_id = create_projection_content(&admin, document).await;
        let published_shell = cms_mutation(
            &admin,
            Method::POST,
            &format!("/content/{shell_id}/publish"),
            1,
            json!({"reason": "Publish the complete site shell fixture"}),
        )
        .await;
        assert_eq!(
            published_shell.status(),
            StatusCode::CREATED,
            "publish {kind} site shell fixture"
        );
    }

    let slug = format!("projection-{}", Uuid::new_v4().simple());
    let id = create_projection_content(&admin, projection_news_document(&slug, true)).await;
    let published = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{id}/publish"),
        1,
        json!({"reason": "Publish the public projection contract fixture"}),
    )
    .await;
    assert_eq!(
        published.status(),
        StatusCode::CREATED,
        "publish projection content"
    );
    let published = response_json(published).await;
    assert_eq!(published["publishedRevision"], 1);

    let canonical: String = sqlx::query_scalar(
        "SELECT canonical_path FROM public_routes WHERE entity_type='content' AND entity_id=$1",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(canonical, format!("/en/resources/news/{slug}"));
    let indexable: bool = sqlx::query_scalar(
        "SELECT indexable FROM public_routes WHERE entity_type='content' AND entity_id=$1",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(
        indexable,
        "indexable content must publish an indexable route"
    );

    let resolved = public_get(
        &app,
        &format!("/api/public/v1/routes/resolve?path={canonical}&locale=en"),
    )
    .await;
    assert_eq!(resolved.status(), StatusCode::OK, "resolve published route");
    let resolved = response_json(resolved).await;
    assert_eq!(resolved["templateKey"], "newsDetail");
    assert_eq!(resolved["entityId"], id);
    assert_eq!(resolved["indexable"], true);
    assert_eq!(resolved["page"]["schemaVersion"], 2);
    assert_eq!(resolved["page"]["title"], format!("Projection {slug}"));
    assert_eq!(resolved["page"]["publishedRevision"], 1);
    assert_eq!(resolved["page"]["composition"]["blocks"][0]["type"], "hero");
    assert!(resolved["page"]["resolvedRelations"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(resolved["page"]["resolvedLinks"]
        .as_array()
        .unwrap()
        .is_empty());

    let news = public_get(&app, "/api/public/v1/news?locale=en").await;
    assert_eq!(news.status(), StatusCode::OK, "list published news");
    let news = response_json(news).await;
    let entry = news["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["content"]["slug"] == slug)
        .expect("published news is listed");
    assert_eq!(entry["content"]["schemaVersion"], 2);
    assert_eq!(entry["category"], "Company");
    assert_eq!(entry["publishedAt"], resolved["page"]["updatedAt"]);

    let shell = public_get(&app, "/api/public/v1/site-bootstrap?locale=en").await;
    assert_eq!(shell.status(), StatusCode::OK, "read the site bootstrap");

    let corrupted_path = format!("/en/resources/news/corrupted-{}", Uuid::new_v4().simple());
    sqlx::query(
        "UPDATE public_routes SET canonical_path=$2 WHERE entity_type='content' AND entity_id=$1",
    )
    .bind(Uuid::parse_str(&id).unwrap())
    .bind(&corrupted_path)
    .execute(pool)
    .await
    .unwrap();
    let corrupted_route = public_get(
        &app,
        &format!("/api/public/v1/routes/resolve?path={corrupted_path}&locale=en"),
    )
    .await;
    assert_eq!(corrupted_route.status(), StatusCode::NOT_FOUND);
    let corrupted_content =
        public_get(&app, &format!("/api/public/v1/content/news/{slug}?locale=en")).await;
    assert_eq!(corrupted_content.status(), StatusCode::NOT_FOUND);
    let discovery = public_get(&app, "/api/public/v1/discovery").await;
    assert_eq!(
        discovery.status(),
        StatusCode::OK,
        "discovery requires a complete published site shell"
    );
    let discovery = response_json(discovery).await;
    assert!(discovery["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["path"] != corrupted_path));

    sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
        .bind(Uuid::parse_str(&id).unwrap())
        .execute(pool)
        .await
        .unwrap();
    let hidden_list = public_get(&app, "/api/public/v1/news?locale=en").await;
    assert_eq!(hidden_list.status(), StatusCode::OK);
    let hidden_list = response_json(hidden_list).await;
    assert!(
        hidden_list["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["content"]["slug"] != slug),
        "a News revision without a public route must not be listed"
    );
    let hidden_detail = public_get(&app, &format!("/api/public/v1/news/{slug}?locale=en")).await;
    assert_eq!(
        hidden_detail.status(),
        StatusCode::NOT_FOUND,
        "a News revision without a public route must not have a detail response"
    );

    drop(admin);
    drop(app);
    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn publishing_blocks_unpublished_content_relations_and_missing_media() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let unpublished_id = create_projection_content(
        &admin,
        projection_news_document(&format!("target-{}", Uuid::new_v4().simple()), true),
    )
    .await;
    let unpublished_id = Uuid::parse_str(&unpublished_id).unwrap();
    let relation_id = Uuid::new_v4();
    let block_id = Uuid::new_v4();
    let mut document =
        projection_news_document(&format!("blocked-{}", Uuid::new_v4().simple()), true);
    document["relations"] = json!([
        {"id": relation_id, "slot": "related", "target": {"targetType": "content", "contentId": unpublished_id}}
    ]);
    document["composition"]["blocks"] = json!([
        {
            "type": "hero",
            "id": Uuid::new_v4(),
            "eyebrow": null,
            "heading": "Blocked",
            "lead": null,
            "media": null,
            "actions": [],
            "variant": "standard"
        },
        {
            "type": "relationCollection",
            "id": block_id,
            "heading": "Related",
            "relationIds": [relation_id],
            "presentation": "cards"
        },
        {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
    ]);
    let blocked_id = create_projection_content(&admin, document).await;
    let blocked = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{blocked_id}/publish"),
        1,
        json!({"reason": "Assert relation publication guard"}),
    )
    .await;
    assert_eq!(blocked.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let blocked = response_json(blocked).await;
    let relation_path = "/relations/0/target/contentId";
    assert_eq!(
        blocked["type"],
        "https://api.airtekpower.example/problems/content_dependency_conflict"
    );
    assert!(blocked["errors"][relation_path][0]
        .as_str()
        .unwrap()
        .starts_with("contentNotPublished:"));
    assert_eq!(blocked["issues"][0]["path"], relation_path);
    assert_eq!(blocked["issues"][0]["failedGate"], "contentPublished");
    assert_eq!(blocked["issues"][0]["targetId"], unpublished_id.to_string());

    let mut asset_document =
        projection_news_document(&format!("media-{}", Uuid::new_v4().simple()), true);
    let media_block_id = Uuid::new_v4();
    asset_document["composition"]["blocks"] = json!([
        {
            "type": "hero",
            "id": Uuid::new_v4(),
            "eyebrow": null,
            "heading": "Media guard",
            "lead": null,
            "media": null,
            "actions": [],
            "variant": "standard"
        },
        {
            "type": "media",
            "id": media_block_id,
            "media": {
                "asset": {"assetId": Uuid::new_v4()},
                "altText": "Publication guard fixture",
                "decorative": false
            },
            "caption": null,
            "layout": "inline"
        },
        {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
    ]);
    let asset_id = create_projection_content(&admin, asset_document).await;
    let rejected = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{asset_id}/publish"),
        1,
        json!({"reason": "Assert media publication guard"}),
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let rejected = response_json(rejected).await;
    let media_path = "/composition/blocks/1/media/asset/assetId";
    assert_eq!(
        rejected["type"],
        "https://api.airtekpower.example/problems/content_dependency_conflict"
    );
    assert!(rejected["errors"][media_path][0]
        .as_str()
        .unwrap()
        .starts_with("mediaMissing:"));
    assert_eq!(rejected["issues"][0]["path"], media_path);
    assert_eq!(rejected["issues"][0]["failedGate"], "mediaExists");
    assert!(rejected["issues"][0]["targetId"].is_string());

    drop(admin);
    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn concurrent_canonical_route_claim_returns_conflict_with_the_existing_entity() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let slug = format!("route-race-{}", Uuid::new_v4().simple());
    let content_id = create_projection_content(&admin, projection_news_document(&slug, true)).await;
    let content_id = Uuid::parse_str(&content_id).unwrap();
    let baseline_latest_revision: i64 =
        sqlx::query_scalar("SELECT latest_revision FROM content_entries WHERE id=$1")
            .bind(content_id)
            .fetch_one(sandbox.pool())
            .await
            .unwrap();
    let path = format!("/en/resources/news/{slug}");
    let owner_id = Uuid::new_v4();
    let mut owner_transaction = sandbox.pool().begin().await.unwrap();
    let owner_backend_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *owner_transaction)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO public_routes
           (id,entity_type,entity_id,locale,canonical_path,indexable)
           VALUES ($1,'product',$2,'en',$3,true)"#,
    )
    .bind(Uuid::new_v4())
    .bind(owner_id)
    .bind(&path)
    .execute(&mut *owner_transaction)
    .await
    .unwrap();

    let publish_admin = admin.clone();
    let publish_path = format!("/content/{content_id}/publish");
    let publish_task = tokio::spawn(async move {
        cms_mutation(
            &publish_admin,
            Method::POST,
            &publish_path,
            1,
            json!({"reason": "Exercise the canonical route race"}),
        )
        .await
    });
    let observer = PgPoolOptions::new()
        .max_connections(1)
        .connect(sandbox.connection_url())
        .await
        .unwrap();
    let race_observed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                       SELECT 1 FROM pg_stat_activity AS activity
                       WHERE $1 = ANY(pg_blocking_pids(activity.pid))
                         AND activity.query LIKE '%INSERT INTO public_routes%'
                   )"#,
            )
            .bind(owner_backend_pid)
            .fetch_one(&observer)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok();
    owner_transaction.commit().await.unwrap();

    let response = tokio::time::timeout(std::time::Duration::from_secs(5), publish_task)
        .await
        .expect("publish request completes after the route owner commits")
        .expect("publish task joins");
    assert!(
        race_observed,
        "publish must reach the unique-index race path"
    );
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let problem = response_json(response).await;
    let detail = problem["detail"].as_str().unwrap();
    assert!(
        detail.contains("product"),
        "detail names the entity type: {detail}"
    );
    assert!(
        detail.contains(&owner_id.to_string()),
        "detail names the existing entity: {detail}"
    );
    let latest_revision: i64 =
        sqlx::query_scalar("SELECT latest_revision FROM content_entries WHERE id=$1")
            .bind(content_id)
            .fetch_one(sandbox.pool())
            .await
            .unwrap();
    assert_eq!(
        latest_revision, baseline_latest_revision,
        "failed publication rolls back its revision"
    );
    let revision_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM content_revisions WHERE content_id=$1")
            .bind(content_id)
            .fetch_one(sandbox.pool())
            .await
            .unwrap();
    assert_eq!(
        revision_count, 0,
        "failed publication leaves no revision row"
    );

    observer.close().await;
    drop(admin);
    sandbox.cleanup().await;
}
