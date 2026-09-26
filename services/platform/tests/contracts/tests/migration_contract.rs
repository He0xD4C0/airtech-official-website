use serde_json::{json, Value};
use uuid::Uuid;

mod support;

// These contracts repeatedly create and remove the complete legacy schema.
// PostgreSQL relation-cache invalidation is database-wide, so concurrent DDL
// in separate schemas can still race with a relation that has just been
// dropped and surface as "could not open relation with OID ...". Keep the
// migration chains serial while leaving the rest of the PostgreSQL contracts
// on Cargo's default test parallelism.
static MIGRATION_CHAIN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn migration_document(suffix: &str, version: i64) -> Value {
    json!({
        "schemaVersion": 2,
        "kind": "news",
        "locale": "en",
        "templateKey": "newsDetail",
        "title": format!("Migration {suffix}"),
        "slug": format!("migration-{suffix}"),
        "summary": null,
        "isPlaceholder": true,
        "typeFields": {
            "type": "news", "category": "Company", "authorDisplayName": "AIRTEKPOWER",
            "publicationAt": null, "cover": null, "featured": false
        },
        "body": {"type": "doc", "content": [{"type": "paragraph"}]},
        "composition": {"blocks": [
            {"type": "hero", "id": Uuid::new_v4(), "eyebrow": null,
             "heading": "Migration", "lead": null, "media": null,
             "actions": [], "variant": "standard"},
            {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
        ]},
        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
        "relations": [],
        "draftVersion": version
    })
}

async fn insert_v16_content(
    pool: &sqlx::PgPool,
    id: Uuid,
    document: Option<&Value>,
    editor_email: &str,
) {
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,title,status,is_placeholder,current_revision,
            published_revision,scheduled_for,payload,updated_at,data_origin,
            template_key,latest_revision,cms_published_revision,cms_created_at,cms_updated_by)
           VALUES ($1,'news',$2,'en','Migration','published',true,1,1,NULL,
                   '{}'::jsonb,now(),'editorial','newsDetail',1,1,now(),$3)"#,
    )
    .bind(id)
    .bind(format!("migration-{}", id.simple()))
    .bind(editor_email)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,
            revision_kind,reason,created_by,created_at)
           VALUES ($1,1,NULL,$2,1,'publish','Migration contract publication',$3,now())"#,
    )
    .bind(id)
    .bind(document)
    .bind(editor_email)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn current_publication_and_private_draft_migrate_without_history() {
    let _migration_guard = MIGRATION_CHAIN_LOCK.lock().await;
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 16).await;
    let pool = sandbox.pool();
    let id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let email = format!("migration-{}@example.com", id.simple());
    sqlx::query(
        r#"INSERT INTO users(id,email,password_hash,display_name,status,created_at,updated_at)
           VALUES ($1,$2,'test-only','Migration owner','active',now(),now())"#,
    )
    .bind(user_id)
    .bind(&email)
    .execute(pool)
    .await
    .unwrap();
    let publication = migration_document(&id.simple().to_string(), 1);
    insert_v16_content(pool, id, Some(&publication), &email).await;
    let mut private = publication.clone();
    private["title"] = json!("Unsaved private successor");
    private["draftVersion"] = json!(2);
    sqlx::query(
        r#"INSERT INTO content_drafts(content_id,draft_version,document,updated_by,updated_at)
           VALUES ($1,2,$2,$3,now())"#,
    )
    .bind(id)
    .bind(&private)
    .bind(&email)
    .execute(pool)
    .await
    .unwrap();

    sandbox.apply_version_range(17, 19).await;
    let migrated: (Value, i64) = sqlx::query_as(
        "SELECT document,publication_version FROM cms_published_content WHERE content_id=$1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(migrated, (publication, 1));
    let migrated_draft: (Value, Option<Uuid>, i64) = sqlx::query_as(
        "SELECT document,owner_user_id,base_publication_version FROM cms_drafts WHERE content_id=$1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(migrated_draft, (private, Some(user_id), 1));
    for removed in [
        "content_revisions",
        "content_drafts",
        "content_preview_snapshots",
    ] {
        let relation: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(removed)
            .fetch_one(pool)
            .await
            .unwrap();
        assert!(relation.is_none(), "{removed} must not survive V18");
    }
    let public_view: Value = sqlx::query_scalar("SELECT payload FROM published_news WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(public_view, migrated.0);
    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn invalid_current_publication_aborts_v17() {
    let _migration_guard = MIGRATION_CHAIN_LOCK.lock().await;
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 16).await;
    let id = Uuid::new_v4();
    insert_v16_content(sandbox.pool(), id, None, "missing@example.com").await;
    let sql = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../migrations/V0017__private_content_drafts.sql"),
    )
    .unwrap();
    let mut transaction = sandbox.pool().begin().await.unwrap();
    let error = sqlx::raw_sql(&sql)
        .execute(&mut *transaction)
        .await
        .unwrap_err();
    assert_eq!(
        error
            .as_database_error()
            .and_then(|value| value.code())
            .as_deref(),
        Some("23514")
    );
    transaction.rollback().await.unwrap();
    sandbox.cleanup().await;
}
