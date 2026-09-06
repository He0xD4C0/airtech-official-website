#[tokio::test]
async fn product_rfq_requires_an_exact_published_product_snapshot() {
    let state = AppState::for_test();
    let id = uuid::Uuid::new_v4();
    let product = Product {
        id,
        stable_id: "validated-stable-id".into(),
        model: Some("VALIDATED-MODEL".into()),
        slug: "validated-product".into(),
        locale: "en".into(),
        family: ProductFamily::Axial,
        subtype: None,
        motor_technology: None,
        title: "Validated published product".into(),
        summary: None,
        seo: Default::default(),
        sort_order: 0,
        related_content_ids: Vec::new(),
        specifications: vec![],
        performance_curves: vec![],
        source_snapshot_id: uuid::Uuid::new_v4(),
        source_revision: "source-revision-1".into(),
        current_revision: 4,
        published_revision: Some(4),
        status: PublicationStatus::Published,
        indexable: true,
        updated_at: chrono::Utc::now(),
    };
    state
        .data
        .write()
        .await
        .published_products
        .insert(id, product);
    let app = build_router(state.clone());
    let body = |stable_id: &str| {
        json!({
            "journey": "product",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "productContext": {
                "productId": id,
                "stableId": stable_id,
                "model": "VALIDATED-MODEL",
                "publishedRevision": 4
            },
            "sourcePath": "/en/request-a-quote/product",
            "locale": "en",
            "consent": true,
            "context": {"application": "Industrial cooling", "quantity": 10}
        })
        .to_string()
    };
    let valid = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "exact-product-context")
                .body(Body::from(body("validated-stable-id")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(valid.status(), StatusCode::CREATED);
    let valid_body = response_json(valid).await;
    state.data.write().await.published_products.remove(&id);
    let replay = app
        .clone()
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "exact-product-context")
                .body(Body::from(body("validated-stable-id")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay).await, valid_body);
    let stale = app
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "stale-product-context")
                .body(Body::from(body("stale-or-forged-id")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn each_non_product_rfq_journey_accepts_only_its_typed_context() {
    let valid_contexts = [
        (
            "selection",
            json!({
                "application": "Industrial cooling",
                "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"},
                "requiredCertifications": ["Customer-specified certification"]
            }),
        ),
        (
            "project",
            json!({
                "application": "Air handling system",
                "projectStage": "Engineering",
                "projectScale": "Multiple air handling units"
            }),
        ),
        (
            "replacement",
            json!({
                "application": "Existing cooling assembly",
                "existingModel": "Nameplate text supplied by buyer",
                "dutyPoint": {"airflow": 850, "airflowUnit": "CFM", "pressure": 320, "pressureUnit": "Pa"},
                "replacementGoal": "Engineering review"
            }),
        ),
    ];
    for (index, (journey, context)) in valid_contexts.into_iter().enumerate() {
        let app = build_router(AppState::for_test());
        let response = app
            .oneshot(
                Request::post("/api/public/v1/rfqs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", format!("typed-rfq-{index:04}"))
                    .body(Body::from(
                        json!({
                            "journey": journey,
                            "contact": {"name": "Buyer", "email": "buyer@example.com"},
                            "sourcePath": format!("/en/request-a-quote/{journey}"),
                            "locale": "en",
                            "consent": true,
                            "context": context
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED, "{journey}");
    }
}

#[tokio::test]
async fn rfq_discriminator_rejects_missing_conditional_invalid_and_unknown_fields() {
    let invalid_requests = [
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {"application": "Industrial cooling"}
        }),
        json!({
            "journey": "project",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/project", "locale": "en", "consent": true,
            "context": {"application": "New air handling system"}
        }),
        json!({
            "journey": "replacement",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/replacement", "locale": "en", "consent": true,
            "context": {
                "application": "Existing cooling assembly",
                "dutyPoint": {"airflow": 850, "airflowUnit": "CFM", "pressure": 320, "pressureUnit": "Pa"}
            }
        }),
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {
                "application": "Industrial cooling", "quantity": 1.5,
                "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
            }
        }),
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {
                "application": "Industrial cooling", "unapprovedField": "bypass",
                "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
            }
        }),
        json!({
            "journey": "selection",
            "contact": {"name": "Buyer", "email": "buyer@example.com"},
            "sourcePath": "/en/request-a-quote/selection", "locale": "en", "consent": true,
            "context": {
                "application": "Industrial cooling",
                "dutyPoint": {"airflow": 0, "airflowUnit": "watts", "pressure": -1, "pressureUnit": "psi"}
            }
        }),
    ];
    for (index, body) in invalid_requests.into_iter().enumerate() {
        let app = build_router(AppState::for_test());
        let response = app
            .oneshot(
                Request::post("/api/public/v1/rfqs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", format!("invalid-typed-rfq-{index:04}"))
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
    }
}
