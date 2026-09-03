#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn platform_settings_compare_and_swap_is_atomic_across_instances() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;

    let first_state = postgres_state(&database_url);
    let second_state = postgres_state(&database_url);
    let before = first_state
        .platform_settings()
        .await
        .expect("settings before concurrent updates");
    let first_value = if before.rfq_retention_days == 401 {
        402
    } else {
        401
    };
    let second_value = if before.rfq_retention_days == 403 {
        404
    } else {
        403
    };
    let first_request_id = Uuid::new_v4();
    let second_request_id = Uuid::new_v4();
    let request = |retention_days: i64, request_id: Uuid| {
        Request::patch("/settings")
            .header(
                "x-airtek-authenticated-actor",
                "settings-contract@example.com",
            )
            .header(
                header::IF_MATCH,
                format!("\"revision-{}\"", before.revision),
            )
            .header("x-request-id", request_id.to_string())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "rfqRetentionDays": retention_days,
                    "reason": "Verify cross-instance atomic settings CAS"
                })
                .to_string(),
            ))
            .unwrap()
    };
    let first = airtek_platform::routes::admin::router().with_state(first_state);
    let second = airtek_platform::routes::admin::router().with_state(second_state);
    let (first_response, second_response) = tokio::join!(
        first.oneshot(request(first_value, first_request_id)),
        second.oneshot(request(second_value, second_request_id)),
    );
    let statuses = [
        first_response.unwrap().status(),
        second_response.unwrap().status(),
    ];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CONFLICT)
            .count(),
        1
    );

    let after = postgres_state(&database_url)
        .platform_settings()
        .await
        .expect("settings after concurrent updates");
    assert_eq!(after.revision, before.revision + 1);
    assert!([first_value, second_value].contains(&after.rfq_retention_days));
    let audit_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM audit_log WHERE action='settings.update' AND request_id = ANY($1)",
    )
    .bind(vec![first_request_id, second_request_id])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn product_publish_requires_an_atomic_accepted_postgres_evidence_chain() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;

    let now = chrono::Utc::now();
    let connector_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let snapshot_id = Uuid::new_v4();
    let product_id = Uuid::new_v4();
    let stable_id = format!("TEST-PUBLISH-GATE-{product_id}");
    let source_revision = format!("test-source-{run_id}");
    let product = Product {
        id: product_id,
        stable_id: stable_id.clone(),
        model: Some(format!("TEST-MODEL-{product_id}")),
        slug: format!("test-publish-gate-{product_id}"),
        locale: "en".into(),
        family: ProductFamily::Axial,
        subtype: None,
        motor_technology: None,
        title: "TEST ONLY Product publish gate".into(),
        summary: None,
        seo: Default::default(),
        sort_order: 0,
        related_content_ids: Vec::new(),
        specifications: vec![],
        performance_curves: vec![],
        source_snapshot_id: snapshot_id,
        source_revision: source_revision.clone(),
        current_revision: 1,
        published_revision: None,
        status: PublicationStatus::Draft,
        indexable: false,
        updated_at: now,
    };
    let product_payload = serde_json::to_value(&product).unwrap();
    let run = SyncRun {
        id: run_id,
        source: "feishu".into(),
        dry_run: false,
        mapping_version: "test-publish-gate-v1".into(),
        status: SyncRunStatus::ReadyToPublish,
        resume_cursor: None,
        records_seen: 1,
        records_valid: 1,
        conflict_count: 0,
        started_at: now,
        completed_at: None,
        error: None,
    };
    sqlx::query(
        "INSERT INTO source_connectors (id,connector_type,display_name,enabled) VALUES ($1,'feishu','TEST ONLY publish gate',true)",
    )
    .bind(connector_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,source,dry_run,mapping_version,status,records_seen,records_valid,
            conflict_count,started_at,payload)
           VALUES ($1,'feishu',false,$2,'readyToPublish',1,1,0,$3,$4)"#,
    )
    .bind(run_id)
    .bind(&run.mapping_version)
    .bind(now)
    .bind(serde_json::to_value(&run).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO source_snapshots
           (id,connector_id,sync_run_id,source_record_id,source_revision,checksum,source_payload,received_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
    )
    .bind(snapshot_id)
    .bind(connector_id)
    .bind(run_id)
    .bind(&stable_id)
    .bind(&source_revision)
    .bind("test-only-checksum")
    .bind(&product_payload)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO staging_records
           (id,sync_run_id,source_snapshot_id,source_record_id,validation_status,
            normalized_payload,validation_errors,created_at)
           VALUES ($1,$2,$3,$4,'valid',$5,'[]'::jsonb,$6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(run_id)
    .bind(snapshot_id)
    .bind(&stable_id)
    .bind(&product_payload)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let seeded = postgres_state(&database_url);
    seeded.persist_product(&product).await.unwrap();
    seeded.hydrate().await.unwrap();
    let app = airtek_platform::routes::admin::router().with_state(seeded);
    let facts_payload_before =
        sqlx::query_scalar::<_, Value>("SELECT payload FROM products WHERE id=$1")
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let presentation_key = format!("postgres-presentation-{product_id}");
    let presentation = app
        .clone()
        .oneshot(
            Request::patch(format!("/products/{product_id}/presentation"))
                .header(
                    "x-airtek-authenticated-actor",
                    "postgres-contract@example.com",
                )
                .header("idempotency-key", &presentation_key)
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "locale": "en",
                        "slug": product.slug,
                        "title": "TEST ONLY portal presentation",
                        "summary": null,
                        "seo": {
                            "title": null,
                            "description": null,
                            "canonicalPath": format!("/en/products/axial/{}", product.slug),
                            "indexable": false
                        },
                        "indexable": false,
                        "sortOrder": 7,
                        "relatedContentIds": [],
                        "reason": "TEST ONLY independent presentation update"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(presentation.status(), StatusCode::OK);
    assert_eq!(
        presentation.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT current_revision FROM products WHERE id=$1",)
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, Value>("SELECT payload FROM products WHERE id=$1")
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        facts_payload_before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_revisions WHERE product_id=$1",)
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT current_revision FROM product_presentation_working WHERE product_id=$1 AND locale='en'",
        )
        .bind(product_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_presentation_revisions WHERE product_id=$1 AND locale='en'",
        )
        .bind(product_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    let publish_request = |key: &str| {
        Request::post(format!("/products/{product_id}/publish"))
            .header(
                "x-airtek-authenticated-actor",
                "postgres-contract@example.com",
            )
            .header("idempotency-key", key)
            .header(header::IF_MATCH, "\"revision-1\"")
            .body(Body::empty())
            .unwrap()
    };
    let valid_idempotency_key = format!("postgres-publish-valid-{product_id}");
    let published = app
        .oneshot(publish_request(&valid_idempotency_key))
        .await
        .unwrap();
    assert_eq!(published.status(), StatusCode::OK);
    let revision_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_revisions WHERE product_id=$1")
            .bind(product_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let outbox_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM outbox_events WHERE topic='public.product.published' AND aggregate_id=$1",
    )
    .bind(product_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision_count, 1);
    assert_eq!(outbox_count, 1);

    sqlx::query(
        "UPDATE products SET status='draft', published_revision=NULL, payload=$2, updated_at=$3 WHERE id=$1",
    )
    .bind(product_id)
    .bind(&product_payload)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE staging_records SET validation_status='conflicted' WHERE source_snapshot_id=$1",
    )
    .bind(snapshot_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sync_conflicts
           (id,sync_run_id,product_id,source_record_id,field_diffs)
           VALUES ($1,$2,$3,$4,'[]'::jsonb)"#,
    )
    .bind(Uuid::new_v4())
    .bind(run_id)
    .bind(product_id)
    .bind(&stable_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO product_temporary_overrides
           (id,product_id,field_path,value,reason,created_at,expires_at)
           VALUES ($1,$2,'specifications.inputPower','100'::jsonb,
                   'TEST ONLY expired override',$3,$4)"#,
    )
    .bind(Uuid::new_v4())
    .bind(product_id)
    .bind(now - chrono::Duration::days(2))
    .bind(now - chrono::Duration::days(1))
    .execute(&pool)
    .await
    .unwrap();
    let blocked_state = postgres_state(&database_url);
    blocked_state.hydrate().await.unwrap();
    let blocked_app = airtek_platform::routes::admin::router().with_state(blocked_state);
    let blocked_idempotency_key = format!("postgres-publish-blocked-{product_id}");
    let blocked = blocked_app
        .oneshot(publish_request(&blocked_idempotency_key))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = blocked.into_body().collect().await.unwrap().to_bytes();
    let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
    for field in [
        "staging.validationStatus",
        "conflicts",
        "temporaryOverrides",
    ] {
        assert!(problem["errors"][field]
            .as_array()
            .is_some_and(|errors| !errors.is_empty()));
    }
    let still_draft = sqlx::query_scalar::<_, String>("SELECT status FROM products WHERE id=$1")
        .bind(product_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(still_draft, "draft");
}
