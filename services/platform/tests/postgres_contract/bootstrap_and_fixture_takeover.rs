#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn site_bootstrap_reads_the_published_postgres_projection() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;
    development_seed::seed(&pool, "postgres-contract")
        .await
        .expect("idempotent development projection seed");

    let state = postgres_state(&database_url);
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
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn development_fixture_editorial_takeover_survives_reseed_and_enters_discovery() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;

    // This contract is deliberately restricted to the disposable integration
    // database. Remove only deterministic fixtures named by their own ledger,
    // including records taken over by an earlier run of this same test.
    sqlx::query(
        r#"DELETE FROM public_routes
           WHERE entity_id IN (SELECT entity_id FROM development_fixture_ledger)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"DELETE FROM content_entries
           WHERE id IN (SELECT entity_id FROM development_fixture_ledger
                        WHERE entity_type IN ('content','news'))"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"DELETE FROM general_information
           WHERE id IN (SELECT entity_id FROM development_fixture_ledger
                        WHERE entity_type='generalInformation')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM development_fixture_ledger")
        .execute(&pool)
        .await
        .unwrap();

    development_seed::seed(&pool, "postgres-contract-seed")
        .await
        .expect("initial development seed");
    let run_id = Uuid::new_v4();
    let state = postgres_state(&database_url);
    let admin = airtek_platform::routes::admin::router().with_state(state.clone());

    for (fixture_key, replacement_title) in [
        ("development/content/home/en", "Editorial Home"),
        ("development/content/navigation/en", "Editorial Navigation"),
        ("development/content/footer/en", "Editorial Footer"),
    ] {
        let row = sqlx::query(
            r#"SELECT entry.id,entry.current_revision,entry.payload
               FROM development_fixture_ledger ledger
               JOIN content_entries entry ON entry.id=ledger.entity_id
               WHERE ledger.fixture_key=$1"#,
        )
        .bind(fixture_key)
        .fetch_one(&pool)
        .await
        .unwrap();
        let id: Uuid = row.try_get("id").unwrap();
        let revision: i64 = row.try_get("current_revision").unwrap();
        let payload: Value = row.try_get("payload").unwrap();
        let updated = direct_admin_mutation(
            &admin,
            Method::PATCH,
            &format!("/content/{id}"),
            Some(revision),
            &format!("fixture-content-update-{run_id}-{id}"),
            Some(editorial_content_draft(&payload, replacement_title)),
        )
        .await;
        assert_eq!(updated.status(), StatusCode::OK);
        let updated = serde_json::from_slice::<Value>(
            &updated.into_body().collect().await.unwrap().to_bytes(),
        )
        .unwrap();
        let editorial_revision = updated["currentRevision"].as_i64().unwrap();
        let published = direct_admin_mutation(
            &admin,
            Method::POST,
            &format!("/content/{id}/publish"),
            Some(editorial_revision),
            &format!("fixture-content-publish-{run_id}-{id}"),
            None,
        )
        .await;
        assert_eq!(published.status(), StatusCode::OK);
    }

    let information = sqlx::query(
        r#"SELECT information.id,information.current_revision,information.payload
           FROM development_fixture_ledger ledger
           JOIN general_information information ON information.id=ledger.entity_id
           WHERE ledger.fixture_key='development/general-information/site/en'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let information_id: Uuid = information.try_get("id").unwrap();
    let information_revision: i64 = information.try_get("current_revision").unwrap();
    let mut information_payload: Value = information.try_get("payload").unwrap();
    information_payload["footerStatement"] = json!("Editorial General Information");
    let information_update = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/general-information/{information_id}"),
        Some(information_revision),
        &format!("fixture-information-update-{run_id}"),
        Some(json!({
            "locale": "en",
            "payload": information_payload,
            "isPlaceholder": false
        })),
    )
    .await;
    assert_eq!(information_update.status(), StatusCode::OK);
    let information_update = serde_json::from_slice::<Value>(
        &information_update
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let information_revision = information_update["currentRevision"].as_i64().unwrap();
    let information_publish = direct_admin_mutation(
        &admin,
        Method::POST,
        &format!("/general-information/{information_id}/publish"),
        Some(information_revision),
        &format!("fixture-information-publish-{run_id}"),
        None,
    )
    .await;
    assert_eq!(information_publish.status(), StatusCode::OK);

    // Editorial ownership is monotonic even when an editor later marks the
    // record as a non-indexable placeholder again.
    let information_payload: Value =
        sqlx::query_scalar("SELECT payload FROM general_information WHERE id=$1")
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let marked_placeholder = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/general-information/{information_id}"),
        Some(information_revision),
        &format!("editorial-information-placeholder-{run_id}"),
        Some(json!({
            "locale": "en",
            "payload": information_payload,
            "isPlaceholder": true
        })),
    )
    .await;
    assert_eq!(marked_placeholder.status(), StatusCode::OK);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT data_origin FROM general_information WHERE id=$1",)
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "editorial"
    );
    let marked_placeholder = serde_json::from_slice::<Value>(
        &marked_placeholder
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let placeholder_revision = marked_placeholder["currentRevision"].as_i64().unwrap();
    let information_payload: Value =
        sqlx::query_scalar("SELECT payload FROM general_information WHERE id=$1")
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let restored_editorial = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/general-information/{information_id}"),
        Some(placeholder_revision),
        &format!("editorial-information-restore-{run_id}"),
        Some(json!({
            "locale": "en",
            "payload": information_payload,
            "isPlaceholder": false
        })),
    )
    .await;
    assert_eq!(restored_editorial.status(), StatusCode::OK);
    let restored_editorial = serde_json::from_slice::<Value>(
        &restored_editorial
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let restored_revision = restored_editorial["currentRevision"].as_i64().unwrap();
    let information_republish = direct_admin_mutation(
        &admin,
        Method::POST,
        &format!("/general-information/{information_id}/publish"),
        Some(restored_revision),
        &format!("editorial-information-republish-{run_id}"),
        None,
    )
    .await;
    assert_eq!(information_republish.status(), StatusCode::OK);

    let news = sqlx::query(
        r#"SELECT entry.id,entry.current_revision,entry.payload,working.category,
                  working.author_display_name,working.publication_at,working.featured
           FROM development_fixture_ledger ledger
           JOIN content_entries entry ON entry.id=ledger.entity_id
           JOIN news_working working ON working.content_id=entry.id
           WHERE ledger.fixture_key='development/news/selection-brief/en'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let news_id: Uuid = news.try_get("id").unwrap();
    let news_revision: i64 = news.try_get("current_revision").unwrap();
    let news_payload: Value = news.try_get("payload").unwrap();
    let news_update = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/news/{news_id}"),
        Some(news_revision),
        &format!("fixture-news-update-{run_id}"),
        Some(json!({
            "content": editorial_content_draft(&news_payload, "Editorial News"),
            "category": news.try_get::<String, _>("category").unwrap(),
            "authorDisplayName": news
                .try_get::<Option<String>, _>("author_display_name")
                .unwrap()
                .unwrap(),
            "coverMediaId": null,
            "publishedAt": news
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("publication_at")
                .unwrap(),
            "featured": news.try_get::<bool, _>("featured").unwrap(),
            "dataClass": "developmentFixture"
        })),
    )
    .await;
    assert_eq!(news_update.status(), StatusCode::OK);
    let news_update = serde_json::from_slice::<Value>(
        &news_update.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(news_update["dataClass"], "editorial");
    let news_revision = news_update["content"]["currentRevision"].as_i64().unwrap();
    let news_publish = direct_admin_mutation(
        &admin,
        Method::POST,
        &format!("/news/{news_id}/publish"),
        Some(news_revision),
        &format!("fixture-news-publish-{run_id}"),
        None,
    )
    .await;
    assert_eq!(news_publish.status(), StatusCode::OK);

    for origin in sqlx::query_scalar::<_, String>(
        r#"SELECT data_origin FROM content_entries
           WHERE id IN (
             SELECT entity_id FROM development_fixture_ledger
             WHERE fixture_key=ANY($1)
           )"#,
    )
    .bind(vec![
        "development/content/home/en",
        "development/content/navigation/en",
        "development/content/footer/en",
        "development/news/selection-brief/en",
    ])
    .fetch_all(&pool)
    .await
    .unwrap()
    {
        assert_eq!(origin, "editorial");
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT data_origin FROM general_information WHERE id=$1",)
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "editorial"
    );

    development_seed::seed(&pool, "postgres-contract-reseed")
        .await
        .expect("editorial takeover must be skipped during reseed");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM content_entries WHERE id=$1")
            .bind(news_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "Editorial News"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT payload->>'footerStatement' FROM general_information WHERE id=$1",
        )
        .bind(information_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "Editorial General Information"
    );

    let discovery = build_router(state)
        .oneshot(
            Request::get("/api/public/v1/discovery")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(discovery.status(), StatusCode::OK);
    let discovery =
        serde_json::from_slice::<Value>(&discovery.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    let paths = discovery["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"/en"));
    assert!(paths.contains(&"/en/resources/news/selection-brief-development-preview"));

    let editorial_locale = format!("x-{}", &run_id.simple().to_string()[..8]);
    let information_payload: Value =
        sqlx::query_scalar("SELECT payload FROM general_information WHERE id=$1")
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let admin_placeholder = direct_admin_mutation(
        &admin,
        Method::POST,
        "/general-information",
        None,
        &format!("admin-placeholder-information-{run_id}"),
        Some(json!({
            "locale": editorial_locale,
            "payload": information_payload,
            "isPlaceholder": true
        })),
    )
    .await;
    assert_eq!(admin_placeholder.status(), StatusCode::CREATED);
    let admin_placeholder = serde_json::from_slice::<Value>(
        &admin_placeholder
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let admin_placeholder_id = Uuid::parse_str(admin_placeholder["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT data_origin FROM general_information WHERE id=$1",)
            .bind(admin_placeholder_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "editorial"
    );
    sqlx::query("DELETE FROM general_information WHERE id=$1")
        .bind(admin_placeholder_id)
        .execute(&pool)
        .await
        .unwrap();
}
