use super::*;

use airtek_runtime::services::development_public_site;

/// The public-site fixture needs its owner account to exist; the administrator
/// seed path is gone, so the test creates the row directly.
async fn ensure_fixture_owner(pool: &sqlx::PgPool) {
    let user_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO users (id,email,password_hash,display_name,status,created_at,updated_at)
           VALUES ($1,'local-admin@airtek.invalid','test-only-not-a-login-hash','AIRTEK Local Administrator','active',now(),now())"#,
    )
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO user_roles(user_id,role_id) SELECT $1,id FROM roles WHERE key='super-admin'",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn public_seed_is_transactional_idempotent_and_refuses_partial_state() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let app = build_router(postgres_state(sandbox.connection_url()));
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/site-bootstrap")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let inventory = airtek_runtime::services::public_site_inventory::inspect(sandbox.pool())
        .await
        .unwrap();
    assert!(inventory["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry["missing"] == true));
    ensure_fixture_owner(sandbox.pool()).await;

    let created = development_public_site::ensure(sandbox.pool(), "local-admin@airtek.invalid")
        .await
        .unwrap();
    assert_eq!(created.status, "created");
    assert_eq!(created.published_entries, 18);
    assert_eq!(created.editorial_replacements, 0);
    let response = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/search?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["total"], 0);
    assert_eq!(count(sandbox.pool(), "content_entries").await, 18);
    assert_eq!(count(sandbox.pool(), "public_routes").await, 15);
    assert_eq!(
        count(sandbox.pool(), "development_fixture_ledger").await,
        18
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_log WHERE action='content.publish'",
        )
        .fetch_one(sandbox.pool())
        .await
        .unwrap(),
        18
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox_events WHERE topic='public.content.published'",
        )
        .fetch_one(sandbox.pool())
        .await
        .unwrap(),
        18
    );

    let repeated = development_public_site::ensure(sandbox.pool(), "local-admin@airtek.invalid")
        .await
        .unwrap();
    assert_eq!(repeated.status, "skippedComplete");

    let home_id: Uuid = sqlx::query_scalar(
        "SELECT entity_id FROM development_fixture_ledger WHERE fixture_key='development/public-site/v1/home/home'",
    )
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"UPDATE cms_published_content
           SET document=jsonb_set(
             jsonb_set(document,'{isPlaceholder}','false'::jsonb),
             '{seo,indexable}','true'::jsonb
           )
           WHERE content_id=$1"#,
    )
    .bind(home_id)
    .execute(sandbox.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE content_entries SET data_origin='editorial',is_placeholder=false WHERE id=$1",
    )
    .bind(home_id)
    .execute(sandbox.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE public_routes SET indexable=true WHERE entity_type='content' AND entity_id=$1",
    )
    .bind(home_id)
    .execute(sandbox.pool())
    .await
    .unwrap();
    sqlx::query("DELETE FROM development_fixture_ledger WHERE entity_id=$1")
        .bind(home_id)
        .execute(sandbox.pool())
        .await
        .unwrap();

    let adopted = development_public_site::ensure(sandbox.pool(), "local-admin@airtek.invalid")
        .await
        .unwrap();
    assert_eq!(adopted.status, "skippedComplete");
    assert_eq!(adopted.editorial_replacements, 1);
    // Add a second editorial record so the first cursor is the bare /en homepage.
    sqlx::raw_sql(
        r#"UPDATE cms_published_content SET document=jsonb_set(jsonb_set(document,'{isPlaceholder}','false'),'{seo,indexable}','true')
           WHERE document->>'templateKey'='productIndex';
           UPDATE content_entries SET data_origin='editorial',is_placeholder=false WHERE template_key='productIndex';
           UPDATE public_routes SET indexable=true WHERE canonical_path='/en/products'"#)
        .execute(sandbox.pool()).await.unwrap();
    let first = app
        .clone()
        .oneshot(
            Request::get("/api/public/v1/search?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first = response_json(first).await;
    assert_eq!(first["total"], 2);
    assert_eq!(first["items"][0]["canonicalPath"], "/en");
    let second = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/public/v1/search?limit=1&cursor={}",
                first["nextCursor"].as_str().unwrap()
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        response_json(second).await["items"][0]["canonicalPath"],
        "/en/products"
    );

    sqlx::query("DELETE FROM public_routes WHERE canonical_path='/en/products/selector'")
        .execute(sandbox.pool())
        .await
        .unwrap();
    let error = development_public_site::ensure(sandbox.pool(), "local-admin@airtek.invalid")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("partial or unknown CMS state"));
    sandbox.cleanup().await;
}

async fn count(pool: &sqlx::PgPool, table: &str) -> i64 {
    assert!(matches!(
        table,
        "content_entries" | "public_routes" | "development_fixture_ledger"
    ));
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
