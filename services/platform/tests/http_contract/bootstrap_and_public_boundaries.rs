#[tokio::test]
async fn password_only_session_is_limited_to_totp_enrollment() {
    let state = AppState::for_test();
    let app = build_router(state);
    let session = setup_admin_without_totp(&app).await;

    let current = app
        .clone()
        .oneshot(
            Request::get("/api/admin/v1/auth/session")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(current.status(), StatusCode::OK);
    let current = response_json(current).await;
    assert_eq!(current["totpEnabled"], false);
    assert_eq!(current["permissions"], json!([]));

    let protected = app
        .oneshot(
            Request::get("/api/admin/v1/content")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(protected.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn media_writer_can_read_the_media_catalogue() {
    let state = AppState::for_test();
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    state
        .data
        .write()
        .await
        .admin_users
        .values_mut()
        .next()
        .expect("bootstrap administrator")
        .permissions = vec!["media.write".into()];

    let response = app
        .oneshot(
            Request::get("/api/admin/v1/media/assets")
            .header(header::COOKIE, &session.cookie)
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "media.write must pass both RBAC layers before the no-PostgreSQL fixture fails"
    );
}

#[tokio::test]
async fn invitation_acceptance_is_unauthenticated_but_admin_origin_bound_and_strictly_typed() {
    let app = build_router(AppState::for_test());
    let body = json!({
        "token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "password": "correct-horse-123"
    });
    let wrong_origin = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/invitations/accept")
                .header(header::ORIGIN, "http://localhost:3000")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_origin.status(), StatusCode::FORBIDDEN);

    let unknown_field = app
        .clone()
        .oneshot(
            Request::post("/api/admin/v1/auth/invitations/accept")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                        "password": "correct-horse-123",
                        "email": "attacker-controlled@example.com"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown_field.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let no_database = app
        .oneshot(
            Request::post("/api/admin/v1/auth/invitations/accept")
                .header(header::ORIGIN, "http://localhost:3100")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_database.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn api_origin_disallows_crawling_and_has_no_sitemaps() {
    let app = build_router(AppState::for_test());
    let robots = app
        .clone()
        .oneshot(Request::get("/robots.txt").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(robots.status(), StatusCode::OK);
    assert_eq!(
        robots.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
    let body = robots.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"User-agent: *\nDisallow: /\n");

    for path in [
        "/sitemap.xml",
        "/sitemap-products.xml",
        "/sitemap-anything.xml",
    ] {
        let response = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            response.headers().get("x-robots-tag").unwrap(),
            "noindex, nofollow, noarchive"
        );
    }
}

#[tokio::test]
#[cfg(not(feature = "devtools"))]
async fn production_contract_does_not_expose_devtools_by_default() {
    let app = build_router(AppState::for_test());
    let response = app
        .clone()
        .oneshot(Request::get("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let document = response_json(response).await;
    let paths = document["paths"].as_object().unwrap();
    assert!(paths.keys().all(|path| !path.contains("devtools")));

    let response = app
        .oneshot(
            Request::post("/api/devtools/v1/sessions/token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[cfg(feature = "devtools")]
async fn development_contract_exposes_one_time_terminal_tokens() {
    let app = build_router(AppState::for_test());
    let response = app
        .clone()
        .oneshot(Request::get("/openapi.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let document = response_json(response).await;
    assert!(document["paths"]
        .as_object()
        .unwrap()
        .contains_key("/api/devtools/v1/terminal"));

    let session = setup_admin(&app).await;
    let response = app
        .oneshot(
            Request::post("/api/devtools/v1/sessions/token")
                .header(header::COOKIE, session.cookie)
                .header("x-csrf-token", session.csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let token = response_json(response).await;
    assert_eq!(token["expiresInSeconds"], 60);
    assert_eq!(token["token"].as_str().unwrap().len(), 32);
}

#[tokio::test]
async fn selector_fails_safe_without_validated_product_data() {
    let app = build_router(AppState::for_test());
    let response = app
        .oneshot(
            Request::post("/api/public/v1/selector")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "airflow": 1000,
                        "airflowUnit": "m3/h",
                        "pressure": 300,
                        "pressureUnit": "Pa",
                        "requiredCertifications": []
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["outcome"], "noValidatedCandidates");
    assert_eq!(body["candidates"], json!([]));
}

fn published_test_product(stable_id: &str, slug: &str, family: ProductFamily) -> Product {
    Product {
        id: uuid::Uuid::new_v4(),
        stable_id: stable_id.into(),
        model: Some(format!("TEST-{stable_id}")),
        slug: slug.into(),
        locale: "en".into(),
        family,
        subtype: None,
        motor_technology: None,
        title: format!("Published {stable_id}"),
        summary: None,
        seo: Default::default(),
        sort_order: 0,
        related_content_ids: Vec::new(),
        specifications: vec![],
        performance_curves: vec![],
        source_snapshot_id: uuid::Uuid::new_v4(),
        source_revision: "validated-source-revision".into(),
        current_revision: 1,
        published_revision: Some(1),
        status: PublicationStatus::Published,
        indexable: true,
        updated_at: chrono::Utc::now(),
    }
}

async fn insert_publishable_product(state: &AppState, product: Product) {
    let now = chrono::Utc::now();
    let run_id = uuid::Uuid::new_v4();
    let snapshot = SourceSnapshot {
        id: product.source_snapshot_id,
        connector_id: uuid::Uuid::new_v4(),
        sync_run_id: run_id,
        source_record_id: product.stable_id.clone(),
        source_revision: product.source_revision.clone(),
        checksum: "test-only-source-checksum".into(),
        source_payload: serde_json::to_value(&product).unwrap(),
        received_at: now,
    };
    let staging = StagingRecord {
        id: uuid::Uuid::new_v4(),
        sync_run_id: run_id,
        source_snapshot_id: snapshot.id,
        source_record_id: snapshot.source_record_id.clone(),
        validation_status: StagingValidationStatus::Valid,
        normalized_payload: Some(serde_json::to_value(&product).unwrap()),
        validation_errors: vec![],
        created_at: now,
    };
    let run = SyncRun {
        id: run_id,
        source: "feishu".into(),
        dry_run: false,
        mapping_version: "test-mapping-v1".into(),
        status: SyncRunStatus::ReadyToPublish,
        resume_cursor: None,
        records_seen: 1,
        records_valid: 1,
        conflict_count: 0,
        started_at: now,
        completed_at: None,
        error: None,
    };
    let mut data = state.data.write().await;
    data.product_revisions
        .entry(product.id)
        .or_default()
        .insert(product.current_revision, product.clone());
    data.source_snapshots.insert(snapshot.id, snapshot);
    data.staging_records.insert(staging.id, staging);
    data.sync_runs.insert(run.id, run);
    data.products.insert(product.id, product);
}
