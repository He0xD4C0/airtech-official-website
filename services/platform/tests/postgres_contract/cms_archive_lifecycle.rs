#[cfg(feature = "devtools")]
async fn cms_archive_with_key(
    app: &Router,
    id: Uuid,
    draft_version: i64,
    key: &str,
) -> Response {
    app.clone()
        .oneshot(
            Request::post(format!("/content/{id}/archive"))
                .header(header::IF_MATCH, format!("\"draft-{draft_version}\""))
                .header("idempotency-key", key)
                .header("x-actor", "postgres-cms@example.com")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "Archive explicitly after verified unpublication"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn archive_never_implicitly_unpublishes_and_requires_an_explicit_restore_before_republish() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);
    let slug = format!("archive-lifecycle-{}", Uuid::new_v4().simple());
    let id = Uuid::parse_str(
        &create_projection_content(&admin, projection_news_document(&slug, true)).await,
    )
    .unwrap();
    let published = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{id}/snapshots"),
        1,
        json!({"intent": "publish", "reason": "Publish archive lifecycle fixture"}),
    )
    .await;
    assert_eq!(published.status(), StatusCode::CREATED);

    let blocked = cms_archive_with_key(&admin, id, 1, &format!("blocked-archive-{id}")).await;
    assert_eq!(blocked.status(), StatusCode::CONFLICT);
    let blocked = response_json(blocked).await;
    assert_eq!(
        blocked["type"],
        "https://api.airtekpower.example/problems/content_must_be_unpublished"
    );
    let still_public: (String, Option<i64>, i64) = sqlx::query_as(
        r#"SELECT status,cms_published_revision,
                  (SELECT count(*) FROM public_routes
                   WHERE entity_type='content' AND entity_id=$1)
           FROM content_entries WHERE id=$1"#,
    )
    .bind(id)
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(still_public, ("published".into(), Some(1), 1));

    let unpublished = cms_unpublish_with_key(
        &admin,
        id,
        1,
        1,
        &format!("unpublish-before-archive-{id}"),
    )
    .await;
    assert_eq!(unpublished.status(), StatusCode::OK);
    let archive_key = format!("archive-after-unpublish-{id}");
    let archived = cms_archive_with_key(&admin, id, 1, &archive_key).await;
    assert_eq!(archived.status(), StatusCode::OK);
    let archived_body = response_json(archived).await;
    assert_eq!(archived_body["status"], "archived");
    assert!(archived_body["publishedRevision"].is_null());
    let replay = cms_archive_with_key(&admin, id, 1, &archive_key).await;
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(response_json(replay).await, archived_body);

    let persisted: (String, Option<i64>, i64, i64, i64) = sqlx::query_as(
        r#"SELECT status,cms_published_revision,
                  (SELECT count(*) FROM public_routes
                   WHERE entity_type='content' AND entity_id=$1),
                  (SELECT count(*) FROM audit_log
                   WHERE action='content.archive' AND entity_id=$1),
                  (SELECT count(*) FROM outbox_events
                   WHERE topic='content.archived' AND aggregate_id=$1)
           FROM content_entries WHERE id=$1"#,
    )
    .bind(id)
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(persisted, ("archived".into(), None, 0, 1, 1));

    let republish = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{id}/snapshots"),
        1,
        json!({"intent": "publish", "reason": "Archived content must not silently revive"}),
    )
    .await;
    assert_eq!(republish.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(republish).await["type"],
        "https://api.airtekpower.example/problems/content_archived"
    );

    drop(admin);
    sandbox.cleanup().await;
}
