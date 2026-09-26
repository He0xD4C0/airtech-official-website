use super::*;

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn public_rate_limit_survives_an_api_restart() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool().clone();
    support::assert_flyway_schema_current(&pool).await;
    let baseline_contacts = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM contact_requests")
        .fetch_one(&pool)
        .await
        .expect("contact baseline");

    let run_id = Uuid::new_v4();
    let unique_source = Ipv6Addr::from(u128::from_be_bytes(*run_id.as_bytes()));
    let peer = SocketAddr::new(unique_source.into(), 41_000);
    let first_state = postgres_direct_media_state(sandbox.connection_url());
    airtek_runtime::services::runtime_preparation::prepare(&first_state.pool)
        .await
        .expect("first runtime preparation");
    let first_app = build_router(first_state);
    for index in 0..5 {
        let response = first_app
            .clone()
            .oneshot(contact_request(index, run_id, peer))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let restarted_state = postgres_direct_media_state(sandbox.connection_url());
    restarted_state
        .verify_runtime_ready()
        .await
        .expect("restart readiness verification");
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

    let contact_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM contact_requests")
        .fetch_one(&pool)
        .await
        .expect("durable contact count");
    assert_eq!(contact_count, baseline_contacts + 5);

    drop(first_app);
    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn retired_mixed_content_route_cannot_write_database() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let app =
        airtek_http::routes::admin::router().with_state(postgres_state(sandbox.connection_url()));
    let response = app
        .oneshot(Request::post("/content").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let content_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM content_entries")
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(content_count, 0);

    sandbox.cleanup().await;
}
