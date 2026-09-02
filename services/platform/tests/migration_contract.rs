use std::{borrow::Cow, str::FromStr};

use airtek_platform::models::SeoMetadata;
use serde_json::{json, Value};
use sqlx::{
    migrate::Migrator,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use uuid::Uuid;

fn migration_range(minimum: i64, maximum: i64, ignore_missing: bool) -> Migrator {
    let all = sqlx::migrate!("./migrations");
    Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|migration| migration.version >= minimum && migration.version <= maximum)
                .cloned()
                .collect(),
        ),
        ignore_missing,
        locking: false,
        no_tx: false,
    }
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn legacy_product_seo_is_normalized_before_becoming_a_public_projection() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public")
        .execute(&admin_pool)
        .await
        .expect("pg_trgm extension");

    let schema = format!("seo_upgrade_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin_pool)
        .await
        .expect("isolated migration schema");
    let search_path = format!("{schema},public");
    let options = PgConnectOptions::from_str(&database_url)
        .expect("PostgreSQL connection options")
        .options([("search_path", search_path)]);
    let isolated_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("isolated PostgreSQL connection");

    migration_range(1, 6, false)
        .run(&isolated_pool)
        .await
        .expect("pre-0007 schema");
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
    .execute(&isolated_pool)
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
    .execute(&isolated_pool)
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
    .execute(&isolated_pool)
    .await
    .unwrap();

    migration_range(7, 8, true)
        .run(&isolated_pool)
        .await
        .expect("0007-0008 upgrade");

    for (table, revision_filter) in [
        ("product_presentation_working", ""),
        ("product_presentation_revisions", " AND revision=1"),
        ("product_localizations", " AND product_revision=1"),
    ] {
        let seo = sqlx::query_scalar::<_, Value>(&format!(
            "SELECT seo_metadata FROM {table} WHERE product_id=$1 AND locale='en'{revision_filter}"
        ))
        .bind(product_id)
        .fetch_one(&isolated_pool)
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
    .execute(&isolated_pool)
    .await
    .expect_err("typed SEO shape is enforced after migration");
    assert_eq!(
        invalid_type
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );

    isolated_pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin_pool)
        .await
        .expect("migration schema cleanup");
}
