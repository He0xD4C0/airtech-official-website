#[cfg(feature = "devtools")]
fn workflow_principal(user_id: Uuid, email: String) -> AdminPrincipal {
    AdminPrincipal {
        user_id,
        display_name: "TEST ONLY Workflow Admin".into(),
        email,
        role: "super-admin".into(),
        permissions: vec![
            "analytics.read".into(),
            "audit.read".into(),
            "content.read".into(),
            "content.write".into(),
            "content.publish".into(),
            "integration.run".into(),
            "operations.run".into(),
            "rfq.read".into(),
            "rfq.read_pii".into(),
            "rfq.assign".into(),
        ],
        session_id: Uuid::new_v4(),
        session_token_hash: vec![1; 32],
        csrf_hash: vec![2; 32],
        totp_enabled: true,
    }
}

#[cfg(feature = "devtools")]
fn workflow_json_request(
    method: Method,
    path: impl AsRef<str>,
    revision: i64,
    key: impl AsRef<str>,
    body: Value,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path.as_ref())
        .header(header::IF_MATCH, format!("\"revision-{revision}\""))
        .header("idempotency-key", key.as_ref())
        .header("x-request-id", Uuid::new_v4().to_string())
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn explicit_content_publish_reports_readiness_and_replays_one_revision() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(postgres_state(sandbox.connection_url()));

    let slug = format!("explicit-publish-{}", Uuid::new_v4().simple());
    let content_id = Uuid::parse_str(
        &create_projection_content(&app, projection_news_document(&slug, true)).await,
    )
    .unwrap();
    let readiness = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/content/{content_id}/publication-readiness"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readiness.status(), StatusCode::OK);
    let readiness = response_json(readiness).await;
    assert_eq!(readiness["ready"], true);
    assert!(readiness["allowedActions"]
        .as_array()
        .unwrap()
        .contains(&json!("publish")));

    let key = format!("explicit-publish-{content_id}");
    let publish_request = || {
        Request::post(format!("/content/{content_id}/publish"))
            .header(header::IF_MATCH, "\"draft-1\"")
            .header("idempotency-key", &key)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"reason": "Publish through the explicit workflow contract"}).to_string(),
            ))
            .unwrap()
    };
    let published = app.clone().oneshot(publish_request()).await.unwrap();
    assert_eq!(published.status(), StatusCode::CREATED);
    let published = response_json(published).await;
    assert_eq!(published["status"], "published");
    assert_eq!(published["publishedRevision"], 1);
    let replay = app.clone().oneshot(publish_request()).await.unwrap();
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay).await["publishedRevision"], 1);

    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn rfq_workflow_separates_pii_enforces_revisions_and_exports_audit() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool();
    let actor_id = Uuid::new_v4();
    let actor_email = format!("workflow-{}@example.com", actor_id.simple());
    sqlx::query(
        r#"INSERT INTO users(id,email,password_hash,display_name,status,totp_confirmed_at)
           VALUES ($1,$2,'test-only','TEST ONLY Workflow Admin','active',now())"#,
    )
    .bind(actor_id)
    .bind(&actor_email)
    .execute(pool)
    .await
    .unwrap();
    let rfq_id = Uuid::new_v4();
    let reference = format!("TEST-RFQ-{}", rfq_id.simple());
    let now = chrono::Utc::now();
    let payload = json!({
        "id": rfq_id,
        "reference": reference,
        "request": {
            "journey": "selection",
            "contact": {"name": "Protected Buyer", "email": "buyer@example.com", "phone": null,
                "company": "Example Industry", "countryOrRegion": "CN"},
            "productContext": null, "sourcePath": "/en/request-a-quote/selection",
            "locale": "en", "consent": true, "context": {}
        },
        "status": "new", "submittedAt": now, "retentionUntil": now + chrono::Duration::days(30)
    });
    sqlx::query(
        r#"INSERT INTO rfq_submissions
           (id,reference,journey,status,source_path,locale,submitted_at,retention_until,payload)
           VALUES ($1,$2,'selection','new','/en/request-a-quote/selection','en',$3,$4,$5)"#,
    )
    .bind(rfq_id)
    .bind(&reference)
    .bind(now)
    .bind(now + chrono::Duration::days(30))
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(workflow_principal(actor_id, actor_email)))
        .with_state(postgres_state(sandbox.connection_url()));

    let listed = app
        .clone()
        .oneshot(
            Request::get(format!("/rfqs?q={reference}&status=new"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    assert_eq!(listed["total"], 1);
    assert!(listed["items"][0].get("email").is_none());

    let pii = app
        .clone()
        .oneshot(
            Request::get(format!("/rfqs/{rfq_id}/pii"))
                .header("x-request-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pii.status(), StatusCode::OK);
    assert!(pii.headers()[header::CACHE_CONTROL]
        .to_str()
        .unwrap()
        .contains("no-store"));
    assert_eq!(response_json(pii).await["email"], "buyer@example.com");

    let assignment_key = format!("assign-{rfq_id}");
    let assignment_body = json!({"assignedTo": actor_id, "reason": "Assign to workflow owner"});
    let assigned = app
        .clone()
        .oneshot(workflow_json_request(
            Method::POST,
            format!("/rfqs/{rfq_id}/assignment"),
            1,
            &assignment_key,
            assignment_body.clone(),
        ))
        .await
        .unwrap();
    assert_eq!(assigned.status(), StatusCode::OK);
    assert_eq!(response_json(assigned).await["revision"], 2);
    let replay = app
        .clone()
        .oneshot(workflow_json_request(
            Method::POST,
            format!("/rfqs/{rfq_id}/assignment"),
            1,
            &assignment_key,
            assignment_body,
        ))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::OK);

    let note = app
        .clone()
        .oneshot(workflow_json_request(
            Method::POST,
            format!("/rfqs/{rfq_id}/notes"),
            2,
            format!("note-{rfq_id}"),
            json!({"body": "Internal immutable note", "reason": "Record qualification context"}),
        ))
        .await
        .unwrap();
    assert_eq!(note.status(), StatusCode::CREATED);
    assert_eq!(response_json(note).await["notes"].as_array().unwrap().len(), 1);

    let stale = app
        .clone()
        .oneshot(workflow_json_request(
            Method::POST,
            format!("/rfqs/{rfq_id}/status"),
            1,
            format!("stale-{rfq_id}"),
            json!({"status": "spam", "reason": "Mark verified unsolicited request"}),
        ))
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(stale).await["type"],
        "https://api.airtekpower.example/problems/business_revision_conflict"
    );

    let csv = app
        .clone()
        .oneshot(
            Request::get("/audit/export.csv?action=business.note.create")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(csv.status(), StatusCode::OK);
    let csv = String::from_utf8(
        csv.into_body().collect().await.unwrap().to_bytes().to_vec(),
    )
    .unwrap();
    assert!(csv.contains("business.note.create"));
    assert!(!csv.contains("buyer@example.com"));

    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn feishu_conflict_resolution_is_controlled_and_idempotent() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool();
    let run_id = Uuid::new_v4();
    let conflict_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,source,dry_run,mapping_version,status,conflict_count,started_at,payload)
           VALUES ($1,'feishu',false,'workflow-v1','awaitingResolution',1,now(),'{}')"#,
    )
    .bind(run_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sync_conflicts
           (id,sync_run_id,source_record_id,field_diffs)
           VALUES ($1,$2,'TEST-SOURCE-1',$3)"#,
    )
    .bind(conflict_id)
    .bind(run_id)
    .bind(json!([{"fieldPath": "model", "baseValue": "A", "localValue": "B",
        "incomingValue": "C", "sourceOwned": true}]))
    .execute(pool)
    .await
    .unwrap();
    let actor_id = Uuid::new_v4();
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(workflow_principal(
            actor_id,
            format!("feishu-{}@example.com", actor_id.simple()),
        )))
        .with_state(postgres_state(sandbox.connection_url()));
    let key = format!("resolve-{conflict_id}");
    let body = json!({
        "decision": "acceptIncoming",
        "reason": "Accept the incoming Product Master value",
        "evidenceReference": null,
        "expiresAt": null
    });
    let request = || {
        workflow_json_request(
            Method::POST,
            format!("/feishu/conflicts/{conflict_id}/resolve"),
            1,
            &key,
            body.clone(),
        )
    };
    let resolved = app.clone().oneshot(request()).await.unwrap();
    assert_eq!(resolved.status(), StatusCode::OK);
    assert_eq!(response_json(resolved).await["revision"], 2);
    assert_eq!(app.clone().oneshot(request()).await.unwrap().status(), StatusCode::OK);
    let status: String = sqlx::query_scalar("SELECT status FROM sync_runs WHERE id=$1")
        .bind(run_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(status, "readyToPublish");
    let open = app
        .oneshot(
            Request::get("/feishu/conflicts?openOnly=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response_json(open).await["total"], 0);

    sandbox.cleanup().await;
}
