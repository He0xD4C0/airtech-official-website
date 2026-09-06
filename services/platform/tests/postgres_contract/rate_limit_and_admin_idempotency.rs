#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn public_rate_limit_survives_an_api_restart() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;
    let baseline_contacts = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM contact_requests")
        .fetch_one(&pool)
        .await
        .expect("contact baseline");

    let run_id = Uuid::new_v4();
    let unique_source = Ipv6Addr::from(u128::from_be_bytes(*run_id.as_bytes()));
    let peer = SocketAddr::new(unique_source.into(), 41_000);
    let first_state = postgres_state(&database_url);
    first_state.hydrate().await.expect("first hydration");
    let first_app = build_router(first_state);
    for index in 0..5 {
        let response = first_app
            .clone()
            .oneshot(contact_request(index, run_id, peer))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let restarted_state = postgres_state(&database_url);
    restarted_state.hydrate().await.expect("restart hydration");
    let restarted_app = build_router(restarted_state.clone());
    let blocked = restarted_app
        .oneshot(contact_request(5, run_id, peer))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        blocked.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );

    // Force the in-process mirror empty: the summary must still read durable
    // COUNT(*) values rather than silently falling back to hydrated details.
    restarted_state.data.write().await.contacts.clear();
    let admin_handlers = airtek_platform::routes::admin::router().with_state(restarted_state);
    let summary = admin_handlers
        .oneshot(
            Request::get("/analytics/summary")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(summary.status(), StatusCode::OK);
    let summary = summary.into_body().collect().await.unwrap().to_bytes();
    let summary: serde_json::Value = serde_json::from_slice(&summary).unwrap();
    assert_eq!(summary["contactCount"], baseline_contacts + 5);
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn admin_idempotency_serializes_instances_and_survives_restart() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;

    let run_id = Uuid::new_v4();
    let slug = format!("postgres-idempotency-{run_id}");
    let key = format!("postgres-admin-idempotency-{run_id}");
    let body = json!({
        "schemaVersion": 2,
        "kind": "article",
        "slug": slug,
        "locale": "en",
        "templateKey": "articleDetail",
        "title": "PostgreSQL cross-instance idempotency",
        "summary": null,
        "body": {"type": "doc", "content": []},
        "typeFields": {
            "type": "article", "category": null, "authorDisplayName": null,
            "publicationAt": null, "cover": null, "featured": false
        },
        "composition": {"blocks": []},
        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
        "relations": [],
        "isPlaceholder": true,
        "draftVersion": 1
    })
    .to_string();
    let request = |body: String| {
        Request::post("/content")
            .header(
                "x-airtek-authenticated-actor",
                "postgres-contract@example.com",
            )
            .header("idempotency-key", &key)
            .header(header::IF_MATCH, "\"draft-0\"")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap()
    };

    let first = airtek_platform::routes::admin::router().with_state(postgres_state(&database_url));
    let second = airtek_platform::routes::admin::router().with_state(postgres_state(&database_url));
    let (first, replay) = tokio::join!(
        first.oneshot(request(body.clone())),
        second.oneshot(request(body.clone())),
    );
    let first = first.unwrap();
    let replay = replay.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    assert_eq!(replay.status(), StatusCode::CREATED);
    let first = first.into_body().collect().await.unwrap().to_bytes();
    let replay = replay.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(first, replay);
    let created: serde_json::Value = serde_json::from_slice(&first).unwrap();
    let content_id = Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let restarted =
        airtek_platform::routes::admin::router().with_state(postgres_state(&database_url));
    let replay_after_restart = restarted
        .clone()
        .oneshot(request(body.clone()))
        .await
        .unwrap();
    assert_eq!(replay_after_restart.status(), StatusCode::CREATED);
    let different = restarted
        .oneshot(request(body.replace(
            "PostgreSQL cross-instance idempotency",
            "Different PostgreSQL request",
        )))
        .await
        .unwrap();
    assert_eq!(different.status(), StatusCode::CONFLICT);

    let content_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM content_entries WHERE id=$1")
            .bind(content_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM audit_log WHERE action='content.create' AND entity_id=$1",
    )
    .bind(content_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(content_count, 1);
    assert_eq!(audit_count, 1);
}
