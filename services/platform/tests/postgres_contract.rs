use std::net::{Ipv6Addr, SocketAddr};

use airtek_platform::{
    build_router,
    models::{Product, ProductFamily, PublicationStatus, SyncRun, SyncRunStatus},
    AppState, Config,
};
use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

fn contact_request(index: usize, run_id: Uuid, peer: SocketAddr) -> Request<Body> {
    let mut request = Request::post("/api/public/v1/contact")
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            "idempotency-key",
            format!("postgres-rate-{run_id}-{index:04}"),
        )
        .header("x-forwarded-for", format!("203.0.113.{}", index + 1))
        .body(Body::from(
            json!({
                "contact": {"name": "Persistence Test", "email": "test@example.com"},
                "topic": "Rate-limit persistence",
                "message": "This record verifies durable public rate limiting.",
                "sourcePath": "/en/company/contact",
                "locale": "en",
                "consent": true
            })
            .to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(peer));
    request
}

fn postgres_state(database_url: &str) -> AppState {
    let mut config = Config::for_test();
    config.database_url = Some(database_url.to_owned());
    AppState::new(config).expect("PostgreSQL test state")
}

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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");
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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");

    let run_id = Uuid::new_v4();
    let slug = format!("postgres-idempotency-{run_id}");
    let key = format!("postgres-admin-idempotency-{run_id}");
    let body = json!({
        "kind": "article",
        "slug": slug,
        "locale": "en",
        "title": "PostgreSQL cross-instance idempotency",
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {"indexable": false},
        "isPlaceholder": true
    })
    .to_string();
    let request = |body: String| {
        Request::post("/content")
            .header(
                "x-airtek-authenticated-actor",
                "postgres-contract@example.com",
            )
            .header("idempotency-key", &key)
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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");

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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");

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
    let published = app
        .oneshot(publish_request("postgres-publish-gate-valid-0001"))
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
    let blocked = blocked_app
        .oneshot(publish_request("postgres-publish-gate-blocked-0001"))
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
