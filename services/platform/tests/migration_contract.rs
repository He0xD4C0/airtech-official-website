#[cfg(feature = "devtools")]
use airtek_platform::models::MigrationPreflightSeverity;
use airtek_platform::models::SeoMetadata;
#[cfg(feature = "devtools")]
use airtek_platform::{
    services::{cms_content, cms_preflight, development_seed},
    AppState, Config,
};
use serde_json::{json, Value};
use uuid::Uuid;

mod support;

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn legacy_product_seo_is_normalized_before_becoming_a_public_projection() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 6).await;
    let isolated_pool = sandbox.pool();
    let product_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    sqlx::query(
        r#"INSERT INTO products
               (id,stable_id,model,slug,locale,family,source_snapshot_id,
                source_revision,status,current_revision,published_revision,indexable,
                payload,updated_at,data_origin,product_import_run_id)
           VALUES ($1,$2,NULL,$3,'en','centrifugal',NULL,'fixture:legacy-seo',
                   'published',1,1,false,'{}'::jsonb,$4,'developmentFixture',NULL)"#,
    )
    .bind(product_id)
    .bind(format!("DEV-FIXTURE-{product_id}"))
    .bind(format!("legacy-seo-{}", product_id.simple()))
    .bind(now)
    .execute(isolated_pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO product_revisions
               (product_id,revision,source_snapshot_id,payload,created_at,data_origin,
                product_import_run_id)
           VALUES ($1,1,NULL,'{}'::jsonb,$2,'developmentFixture',NULL)"#,
    )
    .bind(product_id)
    .bind(now)
    .execute(isolated_pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO product_localizations
               (product_id,product_revision,locale,slug,title,summary,content,
                seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                updated_by,updated_at)
           VALUES ($1,1,'en',$2,'Legacy SEO',NULL,'{}'::jsonb,
                   $3,'verified',true,false,'developmentFixture',
                   'migration-contract',$4)"#,
    )
    .bind(product_id)
    .bind(format!("legacy-seo-{}", product_id.simple()))
    .bind(json!({
        "title": 42,
        "description": ["not", "decodable"],
        "canonicalPath": {"not": "a string"},
        "indexable": "not-a-boolean",
        "futureField": "preserved"
    }))
    .bind(now)
    .execute(isolated_pool)
    .await
    .unwrap();

    sandbox.apply_version_range(7, 8).await;

    for (table, revision_filter) in [
        ("product_presentation_working", ""),
        ("product_presentation_revisions", " AND revision=1"),
        ("product_localizations", " AND product_revision=1"),
    ] {
        let seo = sqlx::query_scalar::<_, Value>(&format!(
            "SELECT seo_metadata FROM {table} WHERE product_id=$1 AND locale='en'{revision_filter}"
        ))
        .bind(product_id)
        .fetch_one(isolated_pool)
        .await
        .unwrap();
        let typed: SeoMetadata = serde_json::from_value(seo.clone())
            .expect("every upgraded SEO projection must deserialize into the Rust contract");
        assert_eq!(typed, SeoMetadata::default());
        assert_eq!(seo["futureField"], "preserved");
    }

    let invalid_type = sqlx::query(
        r#"UPDATE product_localizations
           SET seo_metadata=jsonb_set(seo_metadata,'{title}','42'::jsonb)
           WHERE product_id=$1 AND locale='en'"#,
    )
    .bind(product_id)
    .execute(isolated_pool)
    .await
    .expect_err("typed SEO shape is enforced after migration");
    assert_eq!(
        invalid_type
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );

    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn unified_content_schema_upgrades_existing_rows_and_guards_revisions() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 10).await;
    let pool = sandbox.pool();
    let content_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,title,status,is_placeholder,current_revision,
            published_revision,scheduled_for,payload,updated_at,data_origin)
           VALUES ($1,'news','migration-contract','en','Migration contract',
                   'published',true,1,1,NULL,'{}'::jsonb,$2,'developmentFixture')"#,
    )
    .bind(content_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,created_by,created_at)
           VALUES ($1,1,'{}'::jsonb,'migration-contract',$2)"#,
    )
    .bind(content_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();

    sandbox.apply_version_range(11, 11).await;
    let row: (String, i64, Option<i64>) = sqlx::query_as(
        r#"SELECT template_key,latest_revision,cms_published_revision
           FROM content_entries WHERE id=$1"#,
    )
    .bind(content_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(row, ("newsDetail".into(), 1, Some(1)));

    let document = json!({"schemaVersion": 2, "draftVersion": 1});
    sqlx::query(
        r#"INSERT INTO content_drafts
           (content_id,draft_version,document,updated_by,updated_at)
           VALUES ($1,1,$2,'migration-contract',$3)"#,
    )
    .bind(content_id)
    .bind(&document)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,
            revision_kind,reason,created_by,created_at)
           VALUES ($1,2,NULL,$2,1,'manual','Migration contract snapshot',
                   'migration-contract',$3)"#,
    )
    .bind(content_id)
    .bind(&document)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE content_entries SET latest_revision=2 WHERE id=$1")
        .bind(content_id)
        .execute(pool)
        .await
        .unwrap();

    let immutable = sqlx::query(
        "UPDATE content_revisions SET reason='Mutated revision data' WHERE content_id=$1 AND revision=2",
    )
    .bind(content_id)
    .execute(pool)
    .await
    .expect_err("canonical revisions must be immutable");
    assert_eq!(
        immutable
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("55000")
    );

    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn development_seed_remains_compatible_with_the_unified_schema() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 11).await;

    let report = development_seed::seed(sandbox.pool(), "migration-contract")
        .await
        .expect("development seed after V0011");
    assert_eq!(report.news_count, 6);
    assert_eq!(report.products_created, 0);
    let missing_template_keys: i64 =
        sqlx::query_scalar("SELECT count(*) FROM content_entries WHERE template_key IS NULL")
            .fetch_one(sandbox.pool())
            .await
            .unwrap();
    assert_eq!(missing_template_keys, 0);

    let mut config = Config::for_test();
    config.database_url = Some(sandbox.connection_url().to_owned());
    let state = AppState::new(config).expect("sandbox state");
    let preflight = cms_preflight::run_preflight(sandbox.pool())
        .await
        .expect("CMS preflight query");
    let blockers = preflight
        .issues
        .iter()
        .filter(|issue| issue.severity == MigrationPreflightSeverity::Blocking)
        .map(|issue| {
            format!(
                "{:?} {:?} {:?} {}",
                issue.code,
                issue.entity_id,
                issue.revision,
                issue.json_path.as_deref().unwrap_or("-")
            )
        })
        .collect::<Vec<_>>();
    assert!(preflight.can_migrate, "CMS blockers: {blockers:#?}");
    cms_content::migrate_legacy_content(&state)
        .await
        .expect("development fixtures migrate into the unified CMS");
    let counts: (i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM content_entries),
                  (SELECT count(*) FROM content_drafts)"#,
    )
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(counts.0, counts.1);

    sandbox.cleanup().await;
}
