#[tokio::test]
async fn public_products_use_stable_opaque_keyset_cursors() {
    let state = AppState::for_test();
    let products = [
        published_test_product("stable-c", "product-c", ProductFamily::Axial),
        published_test_product("stable-a", "product-a", ProductFamily::Axial),
        published_test_product("stable-b", "product-b", ProductFamily::Centrifugal),
    ];
    {
        let mut data = state.data.write().await;
        for product in products {
            data.published_products.insert(product.id, product);
        }
    }
    let app = build_router(state.clone());
    let first = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first = response_json(first).await;
    assert_eq!(first["items"][0]["stableId"], "stable-a");
    assert_eq!(first["items"][1]["stableId"], "stable-b");
    let cursor = first["nextCursor"].as_str().unwrap();
    assert!(!cursor.contains("stable-b"));

    let second = app
        .clone()
        .oneshot(
            Request::get(format!("/api/public/v1/products?limit=2&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let second = response_json(second).await;
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["stableId"], "stable-c");
    assert!(second["nextCursor"].is_null());

    let malformed = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products?cursor=not-a-valid-cursor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let wrong_filters = app
        .oneshot(
            Request::get(format!(
                "/api/public/v1/products?family=axial&cursor={cursor}"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_filters.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn public_product_detail_is_bound_to_family_and_slug() {
    let state = AppState::for_test();
    let products = [
        published_test_product("stable-axial", "shared-slug", ProductFamily::Axial),
        published_test_product(
            "stable-centrifugal",
            "shared-slug",
            ProductFamily::Centrifugal,
        ),
    ];
    {
        let mut data = state.data.write().await;
        for product in products {
            data.published_products.insert(product.id, product);
        }
    }
    let app = build_router(state);

    let axial = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products/shared-slug?family=axial")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(axial.status(), StatusCode::OK);
    assert_eq!(response_json(axial).await["stableId"], "stable-axial");

    let centrifugal = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products/shared-slug?family=centrifugal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(centrifugal.status(), StatusCode::OK);
    assert_eq!(
        response_json(centrifugal).await["stableId"],
        "stable-centrifugal"
    );

    let ambiguous = app
        .oneshot(
            Request::get("/api/public/v1/products/shared-slug")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ambiguous.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn product_presentation_has_an_independent_etag_and_never_revisions_facts() {
    let state = AppState::for_test();
    let mut product = published_test_product(
        "presentation-facts-stable",
        "presentation-facts-source",
        ProductFamily::Axial,
    );
    product.status = PublicationStatus::Draft;
    product.published_revision = None;
    product.indexable = false;
    let product_id = product.id;
    insert_publishable_product(&state, product.clone()).await;
    let app = build_router(state.clone());
    let session = setup_admin(&app).await;
    let body = json!({
        "locale": "en",
        "slug": "portal-owned-title",
        "title": "Portal-owned title",
        "summary": "Editorial presentation only",
        "seo": {
            "title": "Portal-owned SEO title",
            "description": "Editorial SEO description",
            "canonicalPath": "/en/products/axial/portal-owned-title",
            "indexable": false
        },
        "indexable": false,
        "sortOrder": 20,
        "relatedContentIds": [],
        "reason": "Test independent presentation revision"
    })
    .to_string();
    let request = |key: &str, revision: i64| {
        Request::patch(format!("/api/admin/v1/products/{product_id}/presentation"))
            .header(header::COOKIE, &session.cookie)
            .header("x-csrf-token", &session.csrf)
            .header("idempotency-key", key)
            .header(header::IF_MATCH, format!("\"revision-{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.clone()))
            .unwrap()
    };

    let updated = app
        .clone()
        .oneshot(request("product-presentation-update-0001", 1))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(
        updated.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let updated = response_json(updated).await;
    assert_eq!(updated["currentRevision"], 1);
    assert_eq!(updated["presentation"]["revision"], 2);
    assert_eq!(updated["presentation"]["publishedRevision"], Value::Null);

    let replay = app
        .clone()
        .oneshot(request("product-presentation-update-0001", 1))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);
    assert_eq!(
        replay.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );

    let stale = app
        .oneshot(request("product-presentation-update-stale", 1))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);

    let data = state.data.read().await;
    assert_eq!(data.products.get(&product_id), Some(&product));
    assert_eq!(
        data.product_revisions
            .get(&product_id)
            .map(std::collections::BTreeMap::len),
        Some(1)
    );
    assert_eq!(
        data.product_presentations
            .get(&(product_id, "en".to_owned()))
            .map(|presentation| presentation.revision),
        Some(2)
    );
}

#[tokio::test]
async fn public_rfq_rejects_attachment_fields() {
    let app = build_router(AppState::for_test());
    let response = app
        .oneshot(
            Request::post("/api/public/v1/rfqs")
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", "rfq-no-files-0001")
                .body(Body::from(
                    json!({
                        "journey": "selection",
                        "contact": {"name": "Buyer", "email": "buyer@example.com"},
                        "sourcePath": "/en/request-a-quote/selection",
                        "locale": "en",
                        "consent": true,
                        "context": {"attachmentName": "private.pdf"}
                    })
                    .to_string(),
                ))
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

#[tokio::test]
async fn public_submissions_are_idempotent() {
    let app = build_router(AppState::for_test());
    let body = json!({
        "journey": "selection",
        "contact": {"name": "Buyer", "email": "buyer@example.com"},
        "sourcePath": "/en/request-a-quote/selection",
        "locale": "en",
        "consent": true,
        "context": {
            "application": "confidentially supplied to sales",
            "dutyPoint": {"airflow": 1200, "airflowUnit": "m3/h", "pressure": 450, "pressureUnit": "Pa"}
        }
    })
    .to_string();
    let request = || {
        Request::post("/api/public/v1/rfqs")
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotency-key", "rfq-idempotent-0001")
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let first = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = response_json(first).await;
    let replay = app.oneshot(request()).await.unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay).await, first_body);
}
