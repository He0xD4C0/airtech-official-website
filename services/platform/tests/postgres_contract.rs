use std::net::{Ipv6Addr, SocketAddr};

#[cfg(feature = "devtools")]
use airtek_platform::services::development_seed;
use airtek_platform::{
    auth::AdminPrincipal,
    build_router,
    models::{
        ContentEntry, ContentKind, Product, ProductFamily, PublicationStatus, RichTextDocument,
        SeoMetadata, SyncRun, SyncRunStatus,
    },
    AppState, Config,
};
use axum::{
    body::Body,
    extract::{connect_info::ConnectInfo, Extension},
    http::{header, Request, StatusCode},
};
#[cfg(feature = "devtools")]
use axum::{http::Method, response::Response, Router};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
#[cfg(feature = "devtools")]
use sqlx::Row;
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

#[cfg(feature = "devtools")]
async fn direct_admin_mutation(
    app: &Router,
    method: Method,
    path: &str,
    revision: Option<i64>,
    idempotency_key: &str,
    body: Option<Value>,
) -> Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("idempotency-key", idempotency_key)
        .header("x-actor", "postgres-contract-editor");
    if let Some(revision) = revision {
        builder = builder.header(header::IF_MATCH, format!("\"revision-{revision}\""));
    }
    let request = if let Some(body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    app.clone().oneshot(request).await.unwrap()
}

#[cfg(feature = "devtools")]
fn editorial_content_draft(payload: &Value, title: &str) -> Value {
    let mut seo = payload["seo"].clone();
    seo["indexable"] = json!(payload["seo"]["canonicalPath"].is_string());
    json!({
        "kind": payload["kind"],
        "slug": payload["slug"],
        "locale": payload["locale"],
        "title": title,
        "summary": payload["summary"],
        "body": payload["body"],
        "seo": seo,
        "isPlaceholder": false
    })
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn site_bootstrap_reads_the_published_postgres_projection() {
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
    development_seed::seed(&pool, "postgres-contract")
        .await
        .expect("idempotent development projection seed");

    let state = postgres_state(&database_url);
    state.hydrate().await.expect("PostgreSQL hydration");
    let response = build_router(state)
        .oneshot(
            Request::get("/api/public/v1/site-bootstrap?locale=en")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let bootstrap: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(bootstrap["generalInformation"].is_object());
    assert!(bootstrap["navigation"].is_object());
    assert!(bootstrap["footer"].is_object());
    assert!(bootstrap["productFamilies"].is_array());
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn development_fixture_editorial_takeover_survives_reseed_and_enters_discovery() {
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

    // This contract is deliberately restricted to the disposable integration
    // database. Remove only deterministic fixtures named by their own ledger,
    // including records taken over by an earlier run of this same test.
    sqlx::query(
        r#"DELETE FROM public_routes
           WHERE entity_id IN (SELECT entity_id FROM development_fixture_ledger)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"DELETE FROM content_entries
           WHERE id IN (SELECT entity_id FROM development_fixture_ledger
                        WHERE entity_type IN ('content','news'))"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"DELETE FROM general_information
           WHERE id IN (SELECT entity_id FROM development_fixture_ledger
                        WHERE entity_type='generalInformation')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM development_fixture_ledger")
        .execute(&pool)
        .await
        .unwrap();

    development_seed::seed(&pool, "postgres-contract-seed")
        .await
        .expect("initial development seed");
    let run_id = Uuid::new_v4();
    let state = postgres_state(&database_url);
    let admin = airtek_platform::routes::admin::router().with_state(state.clone());

    for (fixture_key, replacement_title) in [
        ("development/content/home/en", "Editorial Home"),
        ("development/content/navigation/en", "Editorial Navigation"),
        ("development/content/footer/en", "Editorial Footer"),
    ] {
        let row = sqlx::query(
            r#"SELECT entry.id,entry.current_revision,entry.payload
               FROM development_fixture_ledger ledger
               JOIN content_entries entry ON entry.id=ledger.entity_id
               WHERE ledger.fixture_key=$1"#,
        )
        .bind(fixture_key)
        .fetch_one(&pool)
        .await
        .unwrap();
        let id: Uuid = row.try_get("id").unwrap();
        let revision: i64 = row.try_get("current_revision").unwrap();
        let payload: Value = row.try_get("payload").unwrap();
        let updated = direct_admin_mutation(
            &admin,
            Method::PATCH,
            &format!("/content/{id}"),
            Some(revision),
            &format!("fixture-content-update-{run_id}-{id}"),
            Some(editorial_content_draft(&payload, replacement_title)),
        )
        .await;
        assert_eq!(updated.status(), StatusCode::OK);
        let updated = serde_json::from_slice::<Value>(
            &updated.into_body().collect().await.unwrap().to_bytes(),
        )
        .unwrap();
        let editorial_revision = updated["currentRevision"].as_i64().unwrap();
        let published = direct_admin_mutation(
            &admin,
            Method::POST,
            &format!("/content/{id}/publish"),
            Some(editorial_revision),
            &format!("fixture-content-publish-{run_id}-{id}"),
            None,
        )
        .await;
        assert_eq!(published.status(), StatusCode::OK);
    }

    let information = sqlx::query(
        r#"SELECT information.id,information.current_revision,information.payload
           FROM development_fixture_ledger ledger
           JOIN general_information information ON information.id=ledger.entity_id
           WHERE ledger.fixture_key='development/general-information/site/en'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let information_id: Uuid = information.try_get("id").unwrap();
    let information_revision: i64 = information.try_get("current_revision").unwrap();
    let mut information_payload: Value = information.try_get("payload").unwrap();
    information_payload["footerStatement"] = json!("Editorial General Information");
    let information_update = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/general-information/{information_id}"),
        Some(information_revision),
        &format!("fixture-information-update-{run_id}"),
        Some(json!({
            "locale": "en",
            "payload": information_payload,
            "isPlaceholder": false
        })),
    )
    .await;
    assert_eq!(information_update.status(), StatusCode::OK);
    let information_update = serde_json::from_slice::<Value>(
        &information_update
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let information_revision = information_update["currentRevision"].as_i64().unwrap();
    let information_publish = direct_admin_mutation(
        &admin,
        Method::POST,
        &format!("/general-information/{information_id}/publish"),
        Some(information_revision),
        &format!("fixture-information-publish-{run_id}"),
        None,
    )
    .await;
    assert_eq!(information_publish.status(), StatusCode::OK);

    // Editorial ownership is monotonic even when an editor later marks the
    // record as a non-indexable placeholder again.
    let information_payload: Value =
        sqlx::query_scalar("SELECT payload FROM general_information WHERE id=$1")
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let marked_placeholder = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/general-information/{information_id}"),
        Some(information_revision),
        &format!("editorial-information-placeholder-{run_id}"),
        Some(json!({
            "locale": "en",
            "payload": information_payload,
            "isPlaceholder": true
        })),
    )
    .await;
    assert_eq!(marked_placeholder.status(), StatusCode::OK);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT data_origin FROM general_information WHERE id=$1",)
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "editorial"
    );
    let marked_placeholder = serde_json::from_slice::<Value>(
        &marked_placeholder
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let placeholder_revision = marked_placeholder["currentRevision"].as_i64().unwrap();
    let information_payload: Value =
        sqlx::query_scalar("SELECT payload FROM general_information WHERE id=$1")
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let restored_editorial = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/general-information/{information_id}"),
        Some(placeholder_revision),
        &format!("editorial-information-restore-{run_id}"),
        Some(json!({
            "locale": "en",
            "payload": information_payload,
            "isPlaceholder": false
        })),
    )
    .await;
    assert_eq!(restored_editorial.status(), StatusCode::OK);
    let restored_editorial = serde_json::from_slice::<Value>(
        &restored_editorial
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let restored_revision = restored_editorial["currentRevision"].as_i64().unwrap();
    let information_republish = direct_admin_mutation(
        &admin,
        Method::POST,
        &format!("/general-information/{information_id}/publish"),
        Some(restored_revision),
        &format!("editorial-information-republish-{run_id}"),
        None,
    )
    .await;
    assert_eq!(information_republish.status(), StatusCode::OK);

    let news = sqlx::query(
        r#"SELECT entry.id,entry.current_revision,entry.payload,working.category,
                  working.author_display_name,working.publication_at,working.featured
           FROM development_fixture_ledger ledger
           JOIN content_entries entry ON entry.id=ledger.entity_id
           JOIN news_working working ON working.content_id=entry.id
           WHERE ledger.fixture_key='development/news/selection-brief/en'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let news_id: Uuid = news.try_get("id").unwrap();
    let news_revision: i64 = news.try_get("current_revision").unwrap();
    let news_payload: Value = news.try_get("payload").unwrap();
    let news_update = direct_admin_mutation(
        &admin,
        Method::PATCH,
        &format!("/news/{news_id}"),
        Some(news_revision),
        &format!("fixture-news-update-{run_id}"),
        Some(json!({
            "content": editorial_content_draft(&news_payload, "Editorial News"),
            "category": news.try_get::<String, _>("category").unwrap(),
            "authorDisplayName": news
                .try_get::<Option<String>, _>("author_display_name")
                .unwrap()
                .unwrap(),
            "coverMediaId": null,
            "publishedAt": news
                .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("publication_at")
                .unwrap(),
            "featured": news.try_get::<bool, _>("featured").unwrap(),
            "dataClass": "developmentFixture"
        })),
    )
    .await;
    assert_eq!(news_update.status(), StatusCode::OK);
    let news_update = serde_json::from_slice::<Value>(
        &news_update.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(news_update["dataClass"], "editorial");
    let news_revision = news_update["content"]["currentRevision"].as_i64().unwrap();
    let news_publish = direct_admin_mutation(
        &admin,
        Method::POST,
        &format!("/news/{news_id}/publish"),
        Some(news_revision),
        &format!("fixture-news-publish-{run_id}"),
        None,
    )
    .await;
    assert_eq!(news_publish.status(), StatusCode::OK);

    for origin in sqlx::query_scalar::<_, String>(
        r#"SELECT data_origin FROM content_entries
           WHERE id IN (
             SELECT entity_id FROM development_fixture_ledger
             WHERE fixture_key=ANY($1)
           )"#,
    )
    .bind(vec![
        "development/content/home/en",
        "development/content/navigation/en",
        "development/content/footer/en",
        "development/news/selection-brief/en",
    ])
    .fetch_all(&pool)
    .await
    .unwrap()
    {
        assert_eq!(origin, "editorial");
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT data_origin FROM general_information WHERE id=$1",)
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "editorial"
    );

    development_seed::seed(&pool, "postgres-contract-reseed")
        .await
        .expect("editorial takeover must be skipped during reseed");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT title FROM content_entries WHERE id=$1")
            .bind(news_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "Editorial News"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT payload->>'footerStatement' FROM general_information WHERE id=$1",
        )
        .bind(information_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "Editorial General Information"
    );

    let discovery = build_router(state)
        .oneshot(
            Request::get("/api/public/v1/discovery")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(discovery.status(), StatusCode::OK);
    let discovery =
        serde_json::from_slice::<Value>(&discovery.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    let paths = discovery["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"/en"));
    assert!(paths.contains(&"/en/resources/news/selection-brief-development-preview"));

    let editorial_locale = format!("x-{}", &run_id.simple().to_string()[..8]);
    let information_payload: Value =
        sqlx::query_scalar("SELECT payload FROM general_information WHERE id=$1")
            .bind(information_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let admin_placeholder = direct_admin_mutation(
        &admin,
        Method::POST,
        "/general-information",
        None,
        &format!("admin-placeholder-information-{run_id}"),
        Some(json!({
            "locale": editorial_locale,
            "payload": information_payload,
            "isPlaceholder": true
        })),
    )
    .await;
    assert_eq!(admin_placeholder.status(), StatusCode::CREATED);
    let admin_placeholder = serde_json::from_slice::<Value>(
        &admin_placeholder
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let admin_placeholder_id = Uuid::parse_str(admin_placeholder["id"].as_str().unwrap()).unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT data_origin FROM general_information WHERE id=$1",)
            .bind(admin_placeholder_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "editorial"
    );
    sqlx::query("DELETE FROM general_information WHERE id=$1")
        .bind(admin_placeholder_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn preview_ticket_revalidates_postgres_identity_session_and_permission() {
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

    let user_id = Uuid::new_v4();
    let role_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let suffix = user_id.simple();
    sqlx::query(
        r#"INSERT INTO roles(id,key,display_name,system_role)
           VALUES ($1,$2,'TEST ONLY Preview Reader',false)"#,
    )
    .bind(role_id)
    .bind(format!("preview-reader-{suffix}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO users
           (id,email,password_hash,display_name,status,totp_confirmed_at,created_at,updated_at)
           VALUES ($1,$2,'test-only-not-a-login-hash','TEST ONLY Preview Reader',
                   'active',now(),now(),now())"#,
    )
    .bind(user_id)
    .bind(format!("preview-reader-{suffix}@example.com"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO user_roles(user_id,role_id) VALUES ($1,$2)")
        .bind(user_id)
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_permissions(role_id,permission_key) VALUES ($1,'content.read')")
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO sessions
           (id,user_id,token_hash,csrf_hash,created_at,expires_at,last_seen_at)
           VALUES ($1,$2,$3,$4,now(),now() + interval '1 hour',now())"#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(Sha256::digest(format!("preview-token-{session_id}")).to_vec())
    .bind(Sha256::digest(format!("preview-csrf-{session_id}")).to_vec())
    .execute(&pool)
    .await
    .unwrap();

    let state = postgres_state(&database_url);
    let content = ContentEntry {
        id: Uuid::new_v4(),
        kind: ContentKind::Article,
        slug: format!("preview-pg-{suffix}"),
        locale: "en".into(),
        title: "PostgreSQL-bound private preview".into(),
        summary: None,
        body: RichTextDocument {
            schema_version: 1,
            doc: json!({"type":"doc","content":[]}),
        },
        seo: SeoMetadata::default(),
        status: PublicationStatus::Draft,
        is_placeholder: false,
        current_revision: 1,
        published_revision: None,
        scheduled_for: None,
        updated_at: chrono::Utc::now(),
    };
    state
        .persist_content(&content, "postgres-contract")
        .await
        .unwrap();
    let token = airtek_platform::preview_token::issue(
        state.config.preview_signing_key.as_ref().unwrap(),
        content.id,
        1,
        user_id,
        session_id,
        600,
    )
    .unwrap()
    .token;
    let app = build_router(state);
    let preview_request = || {
        Request::get("/api/public/v1/content-preview")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };

    let valid = app.clone().oneshot(preview_request()).await.unwrap();
    assert_eq!(valid.status(), StatusCode::OK);
    assert_eq!(
        valid.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    assert_eq!(
        valid.headers().get("x-robots-tag").unwrap(),
        "noindex, nofollow, noarchive"
    );

    sqlx::query("UPDATE users SET status='disabled' WHERE id=$1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(preview_request())
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE users SET status='active' WHERE id=$1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM role_permissions WHERE role_id=$1 AND permission_key='content.read'")
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(preview_request())
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    sqlx::query("INSERT INTO role_permissions(role_id,permission_key) VALUES ($1,'content.read')")
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET revoked_at=now() WHERE id=$1")
        .bind(session_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.oneshot(preview_request()).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn invitation_acceptance_is_one_time_atomic_and_hash_only() {
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

    let inviter_id = Uuid::new_v4();
    let suffix = inviter_id.simple();
    sqlx::query(
        r#"INSERT INTO users(id,email,password_hash,display_name,status,created_at,updated_at)
           VALUES ($1,$2,'test-only-not-a-login-hash','TEST ONLY Inviter','active',now(),now())"#,
    )
    .bind(inviter_id)
    .bind(format!("inviter-{suffix}@example.com"))
    .execute(&pool)
    .await
    .unwrap();
    let role_id: Uuid = sqlx::query_scalar("SELECT id FROM roles WHERE key='content-editor'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let insert_invitation = |id: Uuid,
                             email: String,
                             token: String,
                             invited_at: chrono::DateTime<chrono::Utc>,
                             expires_at: chrono::DateTime<chrono::Utc>,
                             status: &'static str| {
        let pool = pool.clone();
        async move {
            let token_hash = Sha256::digest(token.as_bytes()).to_vec();
            if status == "revoked" {
                sqlx::query(
                    r#"INSERT INTO user_invitations
                       (id,email,display_name,locale,status,token_hash,invited_by,invited_at,
                        expires_at,revoked_at,revoked_by,revoke_reason)
                       VALUES ($1,$2,'TEST ONLY Invitee','zh-CN','revoked',$3,$4,$5,$6,$7,$4,
                               'TEST ONLY revoked invitation')"#,
                )
                .bind(id)
                .bind(email)
                .bind(token_hash)
                .bind(inviter_id)
                .bind(invited_at)
                .bind(expires_at)
                .bind(invited_at + chrono::Duration::minutes(1))
                .execute(&pool)
                .await
                .unwrap();
            } else {
                sqlx::query(
                    r#"INSERT INTO user_invitations
                       (id,email,display_name,locale,status,token_hash,invited_by,invited_at,expires_at)
                       VALUES ($1,$2,'TEST ONLY Invitee','zh-CN','pending',$3,$4,$5,$6)"#,
                )
                .bind(id)
                .bind(email)
                .bind(token_hash)
                .bind(inviter_id)
                .bind(invited_at)
                .bind(expires_at)
                .execute(&pool)
                .await
                .unwrap();
            }
            sqlx::query("INSERT INTO user_invitation_roles(invitation_id,role_id) VALUES ($1,$2)")
                .bind(id)
                .bind(role_id)
                .execute(&pool)
                .await
                .unwrap();
        }
    };
    let acceptance_request = |token: &str, password: &str, request_id: Uuid| {
        Request::post("/api/admin/v1/auth/invitations/accept")
            .header(header::ORIGIN, "http://localhost:3100")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-request-id", request_id.to_string())
            .body(Body::from(
                json!({"token": token, "password": password}).to_string(),
            ))
            .unwrap()
    };

    let now = chrono::Utc::now();
    let invitation_id = Uuid::new_v4();
    let email = format!("invitee-{suffix}@example.com");
    let token = URL_SAFE_NO_PAD.encode(Sha256::digest(invitation_id.as_bytes()));
    let password = "new-admin-password-123";
    insert_invitation(
        invitation_id,
        email.clone(),
        token.clone(),
        now,
        now + chrono::Duration::days(7),
        "pending",
    )
    .await;

    let app = build_router(postgres_state(&database_url));
    let request_id = Uuid::new_v4();
    let accepted = app
        .clone()
        .oneshot(acceptance_request(&token, password, request_id))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::CREATED);
    assert_eq!(
        accepted.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );
    assert!(accepted.headers().get(header::SET_COOKIE).is_none());
    let accepted_body = accepted.into_body().collect().await.unwrap().to_bytes();
    let accepted_body: serde_json::Value = serde_json::from_slice(&accepted_body).unwrap();
    assert_eq!(accepted_body["email"], email);
    assert_eq!(accepted_body["status"], "active");
    assert_eq!(accepted_body["roleKeys"], json!(["content-editor"]));
    let user_id = Uuid::parse_str(accepted_body["userId"].as_str().unwrap()).unwrap();

    let (status, password_hash): (String, String) = sqlx::query_as(
        "SELECT status,password_hash FROM users WHERE id=$1 AND lower(email)=lower($2)",
    )
    .bind(user_id)
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "active");
    assert!(password_hash.starts_with("$argon2id$"));
    let role_count: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM user_roles assignment JOIN roles role ON role.id=assignment.role_id
           WHERE assignment.user_id=$1 AND role.key='content-editor'"#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role_count, 1);
    let history_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_status_history WHERE user_id=$1 AND from_status='invited' AND to_status='active' AND request_id=$2",
    )
    .bind(user_id)
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(history_count, 1);
    let audit_payload: String = sqlx::query_scalar(
        "SELECT concat_ws(' ',actor,before_value::text,after_value::text,reason) FROM audit_log WHERE action='identity.invitation.accept' AND entity_id=$1 AND request_id=$2",
    )
    .bind(invitation_id)
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!audit_payload.contains(&token));
    assert!(!audit_payload.contains(password));

    let reused = app
        .clone()
        .oneshot(acceptance_request(
            &token,
            "another-password-456",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(reused.status(), StatusCode::UNAUTHORIZED);

    let expired_id = Uuid::new_v4();
    let expired_token = URL_SAFE_NO_PAD.encode(Sha256::digest(expired_id.as_bytes()));
    insert_invitation(
        expired_id,
        format!("expired-{suffix}@example.com"),
        expired_token.clone(),
        now - chrono::Duration::days(2),
        now - chrono::Duration::days(1),
        "pending",
    )
    .await;
    let expired = app
        .clone()
        .oneshot(acceptance_request(
            &expired_token,
            "expired-password-123",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM user_invitations WHERE id=$1")
            .bind(expired_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "expired"
    );

    let revoked_id = Uuid::new_v4();
    let revoked_token = URL_SAFE_NO_PAD.encode(Sha256::digest(revoked_id.as_bytes()));
    insert_invitation(
        revoked_id,
        format!("revoked-{suffix}@example.com"),
        revoked_token.clone(),
        now,
        now + chrono::Duration::days(7),
        "revoked",
    )
    .await;
    let revoked = app
        .clone()
        .oneshot(acceptance_request(
            &revoked_token,
            "revoked-password-123",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    let conflict_id = Uuid::new_v4();
    let conflict_token = URL_SAFE_NO_PAD.encode(Sha256::digest(conflict_id.as_bytes()));
    insert_invitation(
        conflict_id,
        email,
        conflict_token.clone(),
        now,
        now + chrono::Duration::days(7),
        "pending",
    )
    .await;
    let conflict = app
        .oneshot(acceptance_request(
            &conflict_token,
            "conflict-password-123",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
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
async fn identity_mutations_are_idempotent_and_commit_with_their_audit_records() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");

    let run_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let target_id = Uuid::new_v4();
    let custom_role_id = Uuid::new_v4();
    let actor_label = format!("identity-contract-{run_id}@example.com");
    let target_email = format!("identity-target-{run_id}@example.com");
    let custom_role_key = format!("identity-contract-{}", run_id.simple());
    sqlx::query(
        r#"INSERT INTO users
           (id,email,password_hash,display_name,status,totp_confirmed_at,created_at,updated_at)
           VALUES
           ($1,$2,'test-only-not-a-login-hash','TEST ONLY Identity Actor','active',now(),now(),now()),
           ($3,$4,'test-only-not-a-login-hash','TEST ONLY Identity Target','active',now(),now(),now())"#,
    )
    .bind(actor_id)
    .bind(&actor_label)
    .bind(target_id)
    .bind(&target_email)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO user_roles(user_id,role_id)
           SELECT $1,id FROM roles WHERE key='super-admin'
           UNION ALL
           SELECT $2,id FROM roles WHERE key='content-editor'"#,
    )
    .bind(actor_id)
    .bind(target_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO roles(id,key,display_name,system_role,revision)
           VALUES ($1,$2,'TEST ONLY Mutable Role',false,1)"#,
    )
    .bind(custom_role_id)
    .bind(&custom_role_key)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO role_permissions(role_id,permission_key) VALUES ($1,'audit.read')")
        .bind(custom_role_id)
        .execute(&pool)
        .await
        .unwrap();

    let principal = AdminPrincipal {
        user_id: actor_id,
        display_name: "TEST ONLY Identity Actor".into(),
        email: actor_label.clone(),
        role: "super-admin".into(),
        permissions: vec!["identity.manage".into()],
        session_id: Uuid::new_v4(),
        session_token_hash: vec![1; 32],
        csrf_hash: vec![2; 32],
        totp_enabled: true,
    };
    let app = airtek_platform::routes::admin_data::router()
        .layer(Extension(principal))
        .with_state(postgres_state(&database_url));

    let user_key = format!("identity-user-{run_id}");
    let user_request = |actor: &str, key: &str, revision: i64, display_name: &str| {
        Request::patch(format!("/users/{target_id}"))
            .header("x-airtek-authenticated-actor", actor)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", key)
            .header(header::IF_MATCH, format!("\"revision-{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "displayName": display_name,
                    "reason": "Verify atomic administrator update"
                })
                .to_string(),
            ))
            .unwrap()
    };
    let first_user = app
        .clone()
        .oneshot(user_request(
            &actor_label,
            &user_key,
            1,
            "TEST ONLY Updated Identity Target",
        ))
        .await
        .unwrap();
    assert_eq!(first_user.status(), StatusCode::OK);
    assert_eq!(
        first_user.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let first_user_body = first_user.into_body().collect().await.unwrap().to_bytes();
    let replay_user = app
        .clone()
        .oneshot(user_request(
            &actor_label,
            &user_key,
            1,
            "TEST ONLY Updated Identity Target",
        ))
        .await
        .unwrap();
    assert_eq!(replay_user.status(), StatusCode::OK);
    assert_eq!(
        replay_user.into_body().collect().await.unwrap().to_bytes(),
        first_user_body
    );
    let user_revision: i64 = sqlx::query_scalar("SELECT revision FROM users WHERE id=$1")
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let user_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.user.update' AND entity_id=$1",
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(user_revision, 2);
    assert_eq!(user_audits, 1);

    let role_key = format!("identity-role-{run_id}");
    let role_request = |actor: &str, key: &str, revision: i64, display_name: &str| {
        Request::patch(format!("/roles/{custom_role_id}"))
            .header("x-airtek-authenticated-actor", actor)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", key)
            .header(header::IF_MATCH, format!("\"revision-{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "displayName": display_name,
                    "permissions": ["content.read", "audit.read"],
                    "reason": "Verify atomic role update"
                })
                .to_string(),
            ))
            .unwrap()
    };
    let first_role = app
        .clone()
        .oneshot(role_request(
            &actor_label,
            &role_key,
            1,
            "TEST ONLY Updated Mutable Role",
        ))
        .await
        .unwrap();
    assert_eq!(first_role.status(), StatusCode::OK);
    assert_eq!(
        first_role.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let first_role_body = first_role.into_body().collect().await.unwrap().to_bytes();
    let replay_role = app
        .clone()
        .oneshot(role_request(
            &actor_label,
            &role_key,
            1,
            "TEST ONLY Updated Mutable Role",
        ))
        .await
        .unwrap();
    assert_eq!(replay_role.status(), StatusCode::OK);
    assert_eq!(
        replay_role.into_body().collect().await.unwrap().to_bytes(),
        first_role_body
    );
    let role_revision: i64 = sqlx::query_scalar("SELECT revision FROM roles WHERE id=$1")
        .bind(custom_role_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let role_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.role.update' AND entity_id=$1",
    )
    .bind(custom_role_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role_revision, 2);
    assert_eq!(role_audits, 1);

    let invitation_email = format!("identity-invite-{run_id}@example.com");
    let expired_invitation_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO user_invitations
           (id,email,display_name,locale,status,token_hash,invited_by,invited_at,expires_at)
           VALUES ($1,$2,'TEST ONLY Expired Invitee','zh-CN','pending',$3,$4,
                   now() - interval '8 days',now() - interval '1 day')"#,
    )
    .bind(expired_invitation_id)
    .bind(&invitation_email)
    .bind(Sha256::digest(format!("expired-{run_id}")).to_vec())
    .bind(actor_id)
    .execute(&pool)
    .await
    .unwrap();
    let content_editor_role: Uuid =
        sqlx::query_scalar("SELECT id FROM roles WHERE key='content-editor'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO user_invitation_roles(invitation_id,role_id) VALUES ($1,$2)")
        .bind(expired_invitation_id)
        .bind(content_editor_role)
        .execute(&pool)
        .await
        .unwrap();

    let invitation_key = format!("identity-invitation-{run_id}");
    let invitation_request = || {
        Request::post("/user-invitations")
            .header("x-airtek-authenticated-actor", &actor_label)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", &invitation_key)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "email": invitation_email,
                    "displayName": "TEST ONLY Replacement Invitee",
                    "roleKeys": ["content-editor"]
                })
                .to_string(),
            ))
            .unwrap()
    };
    let invitation = app.clone().oneshot(invitation_request()).await.unwrap();
    assert_eq!(invitation.status(), StatusCode::CREATED);
    assert_eq!(
        invitation.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let invitation: serde_json::Value =
        serde_json::from_slice(&invitation.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    let invitation_id = Uuid::parse_str(invitation["id"].as_str().unwrap()).unwrap();
    let raw_token = invitation["invitationToken"].as_str().unwrap().to_owned();
    let replay_invitation = app.clone().oneshot(invitation_request()).await.unwrap();
    assert_eq!(replay_invitation.status(), StatusCode::CREATED);
    let replay_invitation: serde_json::Value = serde_json::from_slice(
        &replay_invitation
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(replay_invitation["id"], invitation["id"]);
    assert_eq!(
        replay_invitation["invitationToken"].as_str(),
        Some(raw_token.as_str())
    );
    let expired_status: String =
        sqlx::query_scalar("SELECT status FROM user_invitations WHERE id=$1")
            .bind(expired_invitation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_invitations WHERE lower(email)=lower($1) AND status='pending'",
    )
    .bind(&invitation_email)
    .fetch_one(&pool)
    .await
    .unwrap();
    let invitation_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.invitation.create' AND entity_id=$1",
    )
    .bind(invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let expired_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.invitation.expire' AND entity_id=$1",
    )
    .bind(expired_invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let invitation_key_hash = format!("{:x}", Sha256::digest(invitation_key.as_bytes()));
    let stored_replay: serde_json::Value = sqlx::query_scalar(
        "SELECT response_body FROM idempotency_keys WHERE scope='admin.identity.invitation.create' AND key_hash=$1",
    )
    .bind(invitation_key_hash)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(expired_status, "expired");
    assert_eq!(pending_count, 1);
    assert_eq!(invitation_audits, 1);
    assert_eq!(expired_audits, 1);
    assert_eq!(stored_replay["version"], 1);
    assert!(stored_replay["nonce"].is_string());
    assert!(stored_replay["ciphertext"].is_string());
    assert!(stored_replay.get("invitationToken").is_none());
    assert!(!stored_replay.to_string().contains(&raw_token));

    let revoke_key = format!("identity-revoke-invitation-{run_id}");
    let revoke_request = || {
        Request::post(format!("/user-invitations/{invitation_id}/revoke"))
            .header("x-airtek-authenticated-actor", &actor_label)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", &revoke_key)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"reason": "Verify atomic invitation revocation"}).to_string(),
            ))
            .unwrap()
    };
    let revoked = app.clone().oneshot(revoke_request()).await.unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let replay_revoked = app.clone().oneshot(revoke_request()).await.unwrap();
    assert_eq!(replay_revoked.status(), StatusCode::NO_CONTENT);
    let invitation_status: String =
        sqlx::query_scalar("SELECT status FROM user_invitations WHERE id=$1")
            .bind(invitation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let revoke_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.invitation.revoke' AND entity_id=$1",
    )
    .bind(invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(invitation_status, "revoked");
    assert_eq!(revoke_audits, 1);

    let session_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO sessions
           (id,user_id,token_hash,csrf_hash,created_at,expires_at,last_seen_at)
           VALUES ($1,$2,$3,$4,now(),now() + interval '1 day',now())"#,
    )
    .bind(session_id)
    .bind(target_id)
    .bind(Sha256::digest(format!("session-{run_id}")).to_vec())
    .bind(Sha256::digest(format!("csrf-{run_id}")).to_vec())
    .execute(&pool)
    .await
    .unwrap();
    let sessions_revoked = app
        .clone()
        .oneshot(
            Request::delete(format!("/users/{target_id}/sessions"))
                .header("x-airtek-authenticated-actor", &actor_label)
                .header("x-request-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sessions_revoked.status(), StatusCode::NO_CONTENT);
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let session_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.sessions.revoke' AND entity_id=$1",
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked_at.is_some());
    assert_eq!(session_audits, 1);

    // Force audit insertion to fail for a single test actor. The business
    // changes must roll back with the audit row for all three mutation types.
    let failure_actor = format!("identity-audit-failure-{run_id}@example.com");
    let function_name = format!("test_identity_audit_failure_{}", run_id.simple());
    let trigger_name = format!("test_identity_audit_failure_trigger_{}", run_id.simple());
    sqlx::query(&format!(
        r#"CREATE FUNCTION "{function_name}"() RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.actor = '{failure_actor}' THEN
               RAISE EXCEPTION 'TEST ONLY forced identity audit failure';
             END IF;
             RETURN NEW;
           END $$"#,
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        r#"CREATE TRIGGER "{trigger_name}" BEFORE INSERT ON audit_log
           FOR EACH ROW EXECUTE FUNCTION "{function_name}"()"#,
    ))
    .execute(&pool)
    .await
    .unwrap();

    let failed_user = app
        .clone()
        .oneshot(user_request(
            &failure_actor,
            &format!("identity-user-failure-{run_id}"),
            2,
            "TEST ONLY This Must Roll Back",
        ))
        .await
        .unwrap();
    assert_eq!(failed_user.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (user_name, user_revision): (String, i64) =
        sqlx::query_as("SELECT display_name,revision FROM users WHERE id=$1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user_name, "TEST ONLY Updated Identity Target");
    assert_eq!(user_revision, 2);

    let failed_role = app
        .clone()
        .oneshot(role_request(
            &failure_actor,
            &format!("identity-role-failure-{run_id}"),
            2,
            "TEST ONLY This Role Must Roll Back",
        ))
        .await
        .unwrap();
    assert_eq!(failed_role.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (role_name, role_revision): (String, i64) =
        sqlx::query_as("SELECT display_name,revision FROM roles WHERE id=$1")
            .bind(custom_role_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(role_name, "TEST ONLY Updated Mutable Role");
    assert_eq!(role_revision, 2);

    let rollback_session_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO sessions
           (id,user_id,token_hash,csrf_hash,created_at,expires_at,last_seen_at)
           VALUES ($1,$2,$3,$4,now(),now() + interval '1 day',now())"#,
    )
    .bind(rollback_session_id)
    .bind(target_id)
    .bind(Sha256::digest(format!("rollback-session-{run_id}")).to_vec())
    .bind(Sha256::digest(format!("rollback-csrf-{run_id}")).to_vec())
    .execute(&pool)
    .await
    .unwrap();
    let failed_revoke = app
        .oneshot(
            Request::delete(format!("/users/{target_id}/sessions"))
                .header("x-airtek-authenticated-actor", &failure_actor)
                .header("x-request-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed_revoke.status(), StatusCode::SERVICE_UNAVAILABLE);
    let rollback_revoked_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM sessions WHERE id=$1")
            .bind(rollback_session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(rollback_revoked_at.is_none());

    sqlx::query(&format!(r#"DROP TRIGGER "{trigger_name}" ON audit_log"#))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(r#"DROP FUNCTION "{function_name}"()"#))
        .execute(&pool)
        .await
        .unwrap();
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

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn general_information_revision_pointers_are_deferred_and_locale_bound() {
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

    let id = Uuid::new_v4();
    let locale = format!("x-test-{}", &id.simple().to_string()[..8]);
    let now = chrono::Utc::now();
    let mut initial = pool.begin().await.unwrap();
    sqlx::query(
        r#"INSERT INTO general_information
               (id,scope,locale,status,is_placeholder,data_origin,current_revision,
                published_revision,payload,updated_by,updated_at)
           VALUES ($1,'site',$2,'draft',false,'editorial',1,NULL,$3,
                   'postgres-contract',$4)"#,
    )
    .bind(id)
    .bind(&locale)
    .bind(json!({"brandDisplayName": "GI pointer contract"}))
    .bind(now)
    .execute(&mut *initial)
    .await
    .expect("deferred current pointer permits parent-first insertion");
    sqlx::query(
        r#"INSERT INTO general_information_revisions
               (general_information_id,revision,payload,created_by,created_at,
                locale,is_placeholder,data_origin)
           VALUES ($1,1,$2,'postgres-contract',$3,$4,false,'editorial')"#,
    )
    .bind(id)
    .bind(json!({"brandDisplayName": "GI pointer contract"}))
    .bind(now)
    .bind(&locale)
    .execute(&mut *initial)
    .await
    .unwrap();
    initial.commit().await.expect("matching deferred pointer");

    let app = airtek_platform::routes::admin::router().with_state(postgres_state(&database_url));
    let immutable_locale = app
        .oneshot(
            Request::patch(format!("/general-information/{id}"))
                .header(
                    "x-airtek-authenticated-actor",
                    "postgres-contract@example.com",
                )
                .header(
                    "idempotency-key",
                    format!("postgres-general-information-locale-{id}"),
                )
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "locale": format!("{locale}-api-mismatch"),
                        "payload": {
                            "brandName": "AIRTEKPOWER",
                            "brandLine": null,
                            "homePath": "/en",
                            "footerStatement": null,
                            "copyrightText": null,
                            "defaultSeo": {"title": null, "description": null},
                            "organization": {"name": "AIRTEKPOWER"}
                        },
                        "isPlaceholder": false
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(immutable_locale.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = immutable_locale
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let problem: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        problem["errors"]["locale"],
        json!(["Locale is immutable after General Information is created."])
    );
    let unchanged = sqlx::query_as::<_, (String, i64)>(
        "SELECT locale,current_revision FROM general_information WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unchanged, (locale.clone(), 1));

    let wrong_locale = format!("{locale}-other");
    let mut mismatch = pool.begin().await.unwrap();
    sqlx::query(
        r#"INSERT INTO general_information_revisions
               (general_information_id,revision,payload,created_by,created_at,
                locale,is_placeholder,data_origin)
           VALUES ($1,2,$2,'postgres-contract',$3,$4,false,'editorial')"#,
    )
    .bind(id)
    .bind(json!({"brandDisplayName": "Wrong locale"}))
    .bind(now)
    .bind(&wrong_locale)
    .execute(&mut *mismatch)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE general_information SET current_revision=2,published_revision=2 WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *mismatch)
    .await
    .expect("the deferred constraint is checked at commit");
    let mismatch_error = mismatch
        .commit()
        .await
        .expect_err("a pointer cannot resolve to a different revision locale");
    assert_eq!(
        mismatch_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23503")
    );

    sqlx::query("DELETE FROM general_information WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}
