#[cfg(feature = "devtools")]
async fn cms_unpublish_with_key(
    app: &Router,
    id: Uuid,
    draft_version: i64,
    published_revision: i64,
    key: &str,
) -> Response {
    app.clone()
        .oneshot(
            Request::post(format!("/content/{id}/unpublish"))
                .header(header::IF_MATCH, format!("\"draft-{draft_version}\""))
                .header("idempotency-key", key)
                .header("x-actor", "postgres-cms@example.com")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "expectedPublishedRevision": published_revision,
                        "reason": "Remove the reviewed public projection explicitly"
                    })
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
async fn cms_unpublish_blocks_active_reverse_dependencies_then_removes_projection_atomically() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let pool = sandbox.pool();
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let target_slug = format!("dependency-target-{}", Uuid::new_v4().simple());
    let target_id =
        create_projection_content(&admin, projection_news_document(&target_slug, true)).await;
    let target_id = Uuid::parse_str(&target_id).unwrap();
    let target_publish = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{target_id}/publish"),
        1,
        json!({"reason": "Publish dependency target fixture"}),
    )
    .await;
    assert_eq!(target_publish.status(), StatusCode::CREATED);

    let source_slug = format!("dependency-source-{}", Uuid::new_v4().simple());
    let mut source_document = projection_news_document(&source_slug, true);
    source_document["composition"]["blocks"][0]["actions"] = json!([{
        "label": "Read target",
        "target": {"targetType": "content", "contentId": target_id}
    }]);
    let source_id = create_projection_content(&admin, source_document).await;
    let source_id = Uuid::parse_str(&source_id).unwrap();
    let source_publish = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{source_id}/publish"),
        1,
        json!({"reason": "Publish dependency source fixture"}),
    )
    .await;
    assert_eq!(source_publish.status(), StatusCode::CREATED);

    let stored_dependency: (i64, String) = sqlx::query_as(
        r#"SELECT target_content_revision,reference_path
           FROM cms_publication_dependencies
           WHERE source_content_id=$1 AND target_content_id=$2"#,
    )
    .bind(source_id)
    .bind(target_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(stored_dependency.0, 1);
    assert_eq!(
        stored_dependency.1,
        "/composition/blocks/0/actions/0/target/contentId"
    );
    let snapshot_complete: bool =
        sqlx::query_scalar("SELECT cms_publication_dependency_snapshot_complete($1,1)")
            .bind(source_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(snapshot_complete);
    let published_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM outbox_events WHERE topic='public.content.published' AND aggregate_id=ANY($1)",
    )
    .bind(vec![source_id, target_id])
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(published_events, 2);

    let blocked = cms_unpublish_with_key(
        &admin,
        target_id,
        1,
        1,
        &format!("blocked-unpublish-{target_id}"),
    )
    .await;
    assert_eq!(blocked.status(), StatusCode::CONFLICT);
    let blocked = response_json(blocked).await;
    assert_eq!(
        blocked["type"],
        "https://api.airtekpower.example/problems/content_dependency_conflict"
    );
    let detail = blocked["detail"].as_str().unwrap();
    assert!(detail.contains(&source_id.to_string()));
    assert!(detail.contains("revision=1"));
    assert!(detail.contains(&stored_dependency.1));
    let target_route_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public_routes WHERE entity_type='content' AND entity_id=$1",
    )
    .bind(target_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(target_route_count, 1, "blocked unpublish changes nothing");

    let source_key = format!("successful-unpublish-{source_id}");
    let source_unpublished = cms_unpublish_with_key(&admin, source_id, 1, 1, &source_key).await;
    assert_eq!(source_unpublished.status(), StatusCode::OK);
    let source_unpublished = response_json(source_unpublished).await;
    assert_eq!(source_unpublished["status"], "draft");
    assert!(source_unpublished["publishedRevision"].is_null());
    let replay = cms_unpublish_with_key(&admin, source_id, 1, 1, &source_key).await;
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(response_json(replay).await, source_unpublished);

    let target_unpublished = cms_unpublish_with_key(
        &admin,
        target_id,
        1,
        1,
        &format!("successful-unpublish-{target_id}"),
    )
    .await;
    assert_eq!(target_unpublished.status(), StatusCode::OK);
    let target_unpublished = response_json(target_unpublished).await;
    assert_eq!(target_unpublished["status"], "draft");
    assert!(target_unpublished["publishedRevision"].is_null());

    let remaining_routes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public_routes WHERE entity_type='content' AND entity_id=ANY($1)",
    )
    .bind(vec![source_id, target_id])
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(remaining_routes, 0);
    let unpublished_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM outbox_events WHERE topic='public.content.unpublished' AND aggregate_id=ANY($1)",
    )
    .bind(vec![source_id, target_id])
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(unpublished_events, 2);
    let unpublish_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='content.unpublish' AND entity_id=ANY($1)",
    )
    .bind(vec![source_id, target_id])
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(unpublish_audits, 2);

    drop(admin);
    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn blocked_dependency_publication_leaves_no_revision_route_or_side_effects() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let pool = sandbox.pool();
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let missing_target = Uuid::new_v4();
    let mut document = projection_news_document(
        &format!("blocked-dependency-{}", Uuid::new_v4().simple()),
        true,
    );
    document["composition"]["blocks"][0]["actions"] = json!([{
        "label": "Missing target",
        "target": {"targetType": "content", "contentId": missing_target}
    }]);
    let content_id = create_projection_content(&admin, document).await;
    let content_id = Uuid::parse_str(&content_id).unwrap();
    let rejected = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{content_id}/publish"),
        1,
        json!({"reason": "Reject a missing dependency target"}),
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let rejected = response_json(rejected).await;
    let path = "/composition/blocks/0/actions/0/target/contentId";
    assert_eq!(
        rejected["type"],
        "https://api.airtekpower.example/problems/content_dependency_conflict"
    );
    assert!(rejected["errors"][path][0]
        .as_str()
        .unwrap()
        .starts_with("targetMissing:"));
    assert_eq!(rejected["issues"][0]["path"], path);
    assert_eq!(rejected["issues"][0]["failedGate"], "targetIdentity");
    assert_eq!(
        rejected["issues"][0]["targetId"],
        missing_target.to_string()
    );

    let state: (i64, Option<i64>, i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT entry.latest_revision,entry.cms_published_revision,
                  (SELECT count(*) FROM content_revisions WHERE content_id=entry.id),
                  (SELECT count(*) FROM cms_publication_dependency_sets WHERE content_id=entry.id),
                  (SELECT count(*) FROM public_routes WHERE entity_type='content' AND entity_id=entry.id),
                  (SELECT count(*) FROM outbox_events WHERE topic='public.content.published' AND aggregate_id=entry.id)
           FROM content_entries entry WHERE entry.id=$1"#,
    )
    .bind(content_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(state, (0, None, 0, 0, 0, 0));
    let publish_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='content.publish' AND entity_id=$1",
    )
    .bind(content_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(publish_audits, 0);

    drop(admin);
    sandbox.cleanup().await;
}

#[cfg(feature = "devtools")]
struct PublicationMediaFixture {
    asset_id: Uuid,
}

#[cfg(feature = "devtools")]
impl PublicationMediaFixture {
    fn new() -> Self {
        Self {
            asset_id: Uuid::new_v4(),
        }
    }

    async fn insert(&self, pool: &sqlx::PgPool) {
        sqlx::query(
            r#"INSERT INTO media_assets
               (id,storage_key,original_name,media_type,byte_size,checksum,metadata,
                created_at,storage_backend,content_type,uploaded_by)
               VALUES ($1,$2,'publication.png','image/png',8,$3,'{}'::jsonb,
                       now(),'local','image/png','fixture@example.test')"#,
        )
        .bind(self.asset_id)
        .bind(format!("media/{}/publication.png", self.asset_id))
        .bind("a".repeat(64))
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn publication_accepts_an_existing_direct_media_asset() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let media = PublicationMediaFixture::new();
    media.insert(sandbox.pool()).await;
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let mut document = projection_news_document(
        &format!("object-preflight-{}", Uuid::new_v4().simple()),
        true,
    );
    document["composition"]["blocks"]
        .as_array_mut()
        .unwrap()
        .insert(
            1,
            json!({
                "type": "media",
                "id": Uuid::new_v4(),
                "media": {
                    "asset": {"assetId": media.asset_id},
                    "altText": "Publication preflight fixture",
                    "decorative": false
                },
                "caption": null,
                "layout": "inline"
            }),
        );
    let content_id = create_projection_content(&admin, document).await;
    let content_id = Uuid::parse_str(&content_id).unwrap();

    let published = cms_mutation(
        &admin,
        Method::POST,
        &format!("/content/{content_id}/publish"),
        1,
        json!({"reason": "Publish a direct media reference"}),
    )
    .await;
    assert_eq!(published.status(), StatusCode::CREATED);
    let committed: (i64, Option<i64>, i64, i64) = sqlx::query_as(
        r#"SELECT entry.latest_revision,entry.cms_published_revision,
                  (SELECT count(*) FROM content_revisions WHERE content_id=entry.id),
                  (SELECT count(*) FROM public_routes WHERE entity_type='content' AND entity_id=entry.id)
           FROM content_entries entry WHERE entry.id=$1"#,
    )
    .bind(content_id)
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(committed, (1, Some(1), 1, 1));

    drop(admin);
    sandbox.cleanup().await;
}
