#[cfg(feature = "devtools")]
const ROUND_TRIP_KINDS: &[&str] = &[
    "home",
    "page",
    "solution",
    "technology",
    "article",
    "news",
    "faq",
    "caseStudy",
    "download",
    "company",
    "legal",
    "generalInformation",
    "navigation",
    "footer",
];

#[cfg(feature = "devtools")]
fn round_trip_document(kind: &str, suffix: &str) -> Value {
    let shared = json!({
        "schemaVersion": 2,
        "kind": kind,
        "locale": "en",
        "title": format!("{kind} {suffix}"),
        "summary": null,
        "isPlaceholder": true,
        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
        "relations": [],
        "draftVersion": 1
    });
    let body = json!({"type": "doc", "content": [{"type": "paragraph"}]});
    let hero = json!({
        "type": "hero", "id": Uuid::new_v4(), "eyebrow": null, "heading": "Heading",
        "lead": null, "media": null, "actions": [], "variant": "standard"
    });
    let body_block = json!({"type": "body", "id": Uuid::new_v4(), "width": "standard"});
    match kind {
        "home" => merge_json(
            shared,
            json!({
                "slug": format!("home-{suffix}"),
                "templateKey": "home",
                "typeFields": {"type": "home"},
                "body": body,
                "composition": {"blocks": [hero, body_block]}
            }),
        ),
        "page" => merge_json(
            shared,
            json!({
                "slug": format!("page-{suffix}"),
                "templateKey": "productIndex",
                "typeFields": {"type": "page"},
                "body": null,
                "composition": {"blocks": [hero]}
            }),
        ),
        "solution" => editorial_document(kind, suffix, "solutionDetail"),
        "technology" => editorial_document(kind, suffix, "technologyDetail"),
        "article" => editorial_document(kind, suffix, "articleDetail"),
        "news" => editorial_document(kind, suffix, "newsDetail"),
        "caseStudy" => editorial_document(kind, suffix, "caseStudyDetail"),
        "company" => editorial_document(kind, suffix, "about"),
        "faq" => merge_json(
            shared,
            json!({
                "slug": format!("faq-{suffix}"),
                "templateKey": "faqDetail",
                "typeFields": {"type": "faq", "items": [{
                    "id": Uuid::new_v4(),
                    "question": "Question?",
                    "answer": {"type": "doc", "content": []}
                }]},
                "body": null,
                "composition": {"blocks": [
                    hero,
                    {"type": "faqCollection", "id": Uuid::new_v4(), "heading": null}
                ]}
            }),
        ),
        "download" => merge_json(
            shared,
            json!({
                "slug": format!("download-{suffix}"),
                "templateKey": "downloadDetail",
                "typeFields": {"type": "download", "versionLabel": "v1",
                    "resourceType": "Datasheet", "versionNotes": null},
                "body": body,
                "composition": {"blocks": [
                    hero,
                    {"type": "downloadAsset", "id": Uuid::new_v4(),
                     "asset": {"assetId": Uuid::new_v4()},
                     "label": "Datasheet", "description": null}
                ]}
            }),
        ),
        "legal" => merge_json(
            shared,
            json!({
                "slug": format!("legal-{suffix}"),
                "templateKey": "legal",
                "typeFields": {"type": "legal", "effectiveDate": "2026-09-10"},
                "body": body,
                "composition": {"blocks": [body_block]}
            }),
        ),
        "generalInformation" => merge_json(
            shared,
            json!({
                "slug": null,
                "templateKey": "generalInformation",
                "typeFields": {
                    "type": "generalInformation", "organizationName": "AIRTEKPOWER",
                    "brandLine": null, "homePath": "/en", "footerStatement": null,
                    "copyrightTemplate": null,
                    "contact": {"email": null, "phone": null, "addressLines": [],
                        "locality": null, "region": null, "postalCode": null, "countryCode": null},
                    "socialLinks": [],
                    "defaultSeo": {"title": null, "description": null, "indexable": false,
                        "socialImage": null},
                    "productCategories": [], "navigationCta": null
                },
                "body": null,
                "composition": {"blocks": []}
            }),
        ),
        "navigation" => merge_json(
            shared,
            json!({
                "slug": null, "templateKey": "navigation",
                "typeFields": {"type": "navigation", "items": []},
                "body": null, "composition": {"blocks": []}
            }),
        ),
        "footer" => merge_json(
            shared,
            json!({
                "slug": null, "templateKey": "footer",
                "typeFields": {"type": "footer", "columns": [], "legalLinks": []},
                "body": null, "composition": {"blocks": []}
            }),
        ),
        _ => panic!("unsupported round-trip kind: {kind}"),
    }
}

#[cfg(feature = "devtools")]
fn editorial_type_fields(kind: &str) -> Value {
    match kind {
        "article" | "news" => json!({
            "type": kind, "category": "Company", "authorDisplayName": "AIRTEKPOWER",
            "publicationAt": null, "cover": null, "featured": false
        }),
        "solution" | "technology" => json!({"type": kind, "key": null}),
        "caseStudy" => json!({"type": "caseStudy", "industry": "Manufacturing", "location": "CN"}),
        "company" => json!({"type": "company"}),
        _ => panic!("unsupported editorial kind: {kind}"),
    }
}

#[cfg(feature = "devtools")]
fn editorial_document(kind: &str, suffix: &str, template: &str) -> Value {
    merge_json(
        json!({
            "schemaVersion": 2,
            "kind": kind,
            "locale": "en",
            "title": format!("{kind} {suffix}"),
            "summary": null,
            "isPlaceholder": true,
            "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
            "relations": [],
            "draftVersion": 1
        }),
        json!({
            "slug": format!("{}-{suffix}", kind.to_lowercase()),
            "templateKey": template,
            "typeFields": editorial_type_fields(kind),
            "body": {"type": "doc", "content": [{"type": "paragraph"}]},
            "composition": {"blocks": [
                {"type": "hero", "id": Uuid::new_v4(), "eyebrow": null, "heading": "Heading",
                 "lead": null, "media": null, "actions": [], "variant": "standard"},
                {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
            ]}
        }),
    )
}

#[cfg(feature = "devtools")]
#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn unified_cms_round_trips_every_content_kind_without_loss() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let state = postgres_state(sandbox.connection_url());
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    for kind in ROUND_TRIP_KINDS {
        let created = cms_mutation(
            &app,
            Method::POST,
            "/content",
            0,
            round_trip_document(kind, "rt"),
        )
        .await;
        let created_status = created.status();
        if created_status != StatusCode::CREATED {
            let problem = response_json(created).await;
            panic!("create {kind}: {created_status} {problem}");
        }
        let created = response_json(created).await;
        let id = created["id"].as_str().expect("content id").to_owned();
        let identity = created["draft"].clone();

        let mut saved_document = identity.clone();
        saved_document["title"] = json!(format!("Saved {kind}"));
        let saved = cms_mutation(
            &app,
            Method::PATCH,
            &format!("/content/{id}/draft"),
            1,
            saved_document.clone(),
        )
        .await;
        let saved_status = saved.status();
        if saved_status != StatusCode::OK {
            let problem = response_json(saved).await;
            panic!("save {kind}: {saved_status} {problem}");
        }
        let mut expected = saved_document;
        expected["draftVersion"] = json!(2);
        let saved = response_json(saved).await;
        assert_eq!(saved["draft"], expected, "saved payload {kind}");

        let (status, fetched) = admin_get_json(&app, &format!("/content/{id}/draft")).await;
        assert_eq!(status, StatusCode::OK, "reload {kind}");
        assert_eq!(
            fetched["draft"], expected,
            "reload must return the saved draft unchanged for {kind}"
        );
        assert_eq!(fetched["draft"]["kind"], *kind);
        assert_eq!(fetched["draft"]["locale"], "en");

        // The editor cannot switch content type, locale, or template: the
        // draft endpoint rejects every identity mutation and persists nothing.
        let alternate_kind = if *kind == "home" { "page" } else { "home" };
        for (field, value) in [
            ("kind", json!(alternate_kind)),
            ("locale", json!("de")),
            ("templateKey", json!("compare")),
        ] {
            let mut mutated = expected.clone();
            mutated[field] = value;
            let response = cms_mutation(
                &app,
                Method::PATCH,
                &format!("/content/{id}/draft"),
                2,
                mutated,
            )
            .await;
            let status = response.status();
            if status != StatusCode::UNPROCESSABLE_ENTITY {
                let problem = response_json(response).await;
                panic!("identity {field} must be immutable for {kind}: {status} {problem}");
            }
        }
        let (status, after) = admin_get_json(&app, &format!("/content/{id}/draft")).await;
        assert_eq!(status, StatusCode::OK, "reload after rejection {kind}");
        assert_eq!(
            after["draft"], expected,
            "rejected identity mutations must not change the stored draft {kind}"
        );
    }

    sandbox.cleanup().await;
}
