#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn site_bootstrap_reads_the_published_postgres_projection() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool().clone();
    support::assert_flyway_schema_current(&pool).await;
    development_seed::seed(&pool, "postgres-contract")
        .await
        .expect("idempotent development projection seed");

    let state = postgres_state(sandbox.connection_url());
    state.hydrate().await.expect("PostgreSQL hydration");
    let response = build_router(state)
        .oneshot(
            Request::get("/api/public/v1/site-bootstrap?locale=en")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let bootstrap: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(bootstrap["generalInformation"].is_object());
    assert!(bootstrap["navigation"].is_object());
    assert!(bootstrap["footer"].is_object());
    assert!(bootstrap["productFamilies"].is_array());

    sandbox.cleanup().await;
}
