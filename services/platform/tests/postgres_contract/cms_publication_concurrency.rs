type PublicationRaceState = (
    Option<i64>,
    Option<i64>,
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
    i64,
);

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn concurrent_source_publish_and_target_unpublish_have_one_atomic_winner() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let setup_admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(postgres_state(sandbox.connection_url()));

    let target_slug = format!("race-target-{}", Uuid::new_v4().simple());
    let target_id =
        create_projection_content(&setup_admin, projection_news_document(&target_slug, true)).await;
    let target_id = Uuid::parse_str(&target_id).unwrap();
    let target_publish = cms_mutation(
        &setup_admin,
        Method::POST,
        &format!("/content/{target_id}/publish"),
        1,
        json!({"reason": "Publish concurrent target fixture"}),
    )
    .await;
    assert_eq!(target_publish.status(), StatusCode::CREATED);

    let source_slug = format!("race-source-{}", Uuid::new_v4().simple());
    let mut source_document = projection_news_document(&source_slug, true);
    source_document["composition"]["blocks"][0]["actions"] = json!([{
        "label": "Concurrent target",
        "target": {"targetType": "content", "contentId": target_id}
    }]);
    let source_id = create_projection_content(&setup_admin, source_document).await;
    let source_id = Uuid::parse_str(&source_id).unwrap();
    drop(setup_admin);

    // Distinct states create distinct pools/sessions. The barrier makes both
    // mutations eligible to begin together; the production SERIALIZABLE lock
    // protocol decides which transaction observes and commits first.
    let publish_admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(postgres_state(sandbox.connection_url()));
    let unpublish_admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(postgres_state(sandbox.connection_url()));
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(3));
    let publish_barrier = barrier.clone();
    let publish_task = tokio::spawn(async move {
        publish_barrier.wait().await;
        cms_mutation(
            &publish_admin,
            Method::POST,
            &format!("/content/{source_id}/publish"),
            1,
            json!({"reason": "Race target unpublication"}),
        )
        .await
    });
    let unpublish_barrier = barrier.clone();
    let unpublish_task = tokio::spawn(async move {
        unpublish_barrier.wait().await;
        cms_unpublish_with_key(
            &unpublish_admin,
            target_id,
            1,
            1,
            &format!("concurrent-target-unpublish-{target_id}"),
        )
        .await
    });
    barrier.wait().await;
    let (publish_response, unpublish_response) = tokio::join!(publish_task, unpublish_task);
    let publish_response = publish_response.expect("source publish task joins");
    let unpublish_response = unpublish_response.expect("target unpublish task joins");
    let publish_status = publish_response.status();
    let unpublish_status = unpublish_response.status();

    assert_eq!(
        usize::from(publish_status.is_success()) + usize::from(unpublish_status.is_success()),
        1,
        "only one concurrent mutation may commit: publish={publish_status}, unpublish={unpublish_status}"
    );
    let publish_won = publish_status == StatusCode::CREATED;
    if publish_won {
        assert_eq!(unpublish_status, StatusCode::CONFLICT);
        let problem = response_json(unpublish_response).await;
        assert_eq!(
            problem["type"],
            "https://api.airtekpower.example/problems/content_dependency_conflict"
        );
        assert!(problem["detail"]
            .as_str()
            .unwrap()
            .contains(&source_id.to_string()));
    } else {
        assert_eq!(unpublish_status, StatusCode::OK);
        assert_eq!(publish_status, StatusCode::UNPROCESSABLE_ENTITY);
        let problem = response_json(publish_response).await;
        assert_eq!(
            problem["type"],
            "https://api.airtekpower.example/problems/content_dependency_conflict"
        );
        assert!(problem["issues"].as_array().unwrap().iter().any(|issue| {
            issue["code"] == "contentNotPublished" && issue["targetId"] == target_id.to_string()
        }));
    }

    let state: PublicationRaceState = sqlx::query_as(
        r#"SELECT source.cms_published_revision,target.cms_published_revision,
                  source.status,target.status,
                  (SELECT count(*) FROM content_revisions WHERE content_id=source.id),
                  (SELECT count(*) FROM cms_publication_dependency_sets WHERE content_id=source.id),
                  (SELECT count(*) FROM cms_publication_dependencies
                   WHERE source_content_id=source.id AND target_content_id=target.id),
                  (SELECT count(*) FROM public_routes
                   WHERE entity_type='content' AND entity_id=source.id),
                  (SELECT count(*) FROM public_routes
                   WHERE entity_type='content' AND entity_id=target.id),
                  (SELECT count(*) FROM outbox_events
                   WHERE topic='public.content.unpublished' AND aggregate_id=target.id)
           FROM content_entries source
           CROSS JOIN content_entries target
           WHERE source.id=$1 AND target.id=$2"#,
    )
    .bind(source_id)
    .bind(target_id)
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    if publish_won {
        assert_eq!(
            state,
            (
                Some(1),
                Some(1),
                "published".into(),
                "published".into(),
                1,
                1,
                1,
                1,
                1,
                0,
            )
        );
    } else {
        assert_eq!(
            state,
            (None, None, "draft".into(), "draft".into(), 0, 0, 0, 0, 0, 1,)
        );
    }

    sandbox.cleanup().await;
}
