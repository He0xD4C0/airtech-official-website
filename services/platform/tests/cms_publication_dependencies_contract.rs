#![allow(dead_code, unused_imports)]

use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

mod models {
    pub use airtek_platform::models::{ContentDraftV2, CMS_V2_SCHEMA_VERSION};
}

#[path = "../src/services/cms_publication_dependencies.rs"]
mod dependencies;
mod support;

use dependencies::{
    extract_document, insert_snapshot, lock_targets, validate_document, validate_extracted,
    DependencyIssueCode, PublicationDependencyKind, PublicationDependencyTarget,
};

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn exact_targets_validate_and_persist_when_media_exists() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let pool = sandbox.pool();

    let source_id = Uuid::new_v4();
    let target_content_id = Uuid::new_v4();
    let target_product_id = Uuid::new_v4();
    let asset_id = Uuid::new_v4();
    let original_id = Uuid::new_v4();
    let display_id = Uuid::new_v4();
    let document = dependency_document(target_content_id, target_product_id, asset_id, original_id);
    insert_content(pool, source_id, "en", "draft", &document, false).await;
    insert_content(
        pool,
        target_content_id,
        "en",
        "published",
        &empty_document("en"),
        true,
    )
    .await;
    sqlx::query(
        r#"INSERT INTO public_routes
           (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
           VALUES ($1,'content',$2,'en',$3,false,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(target_content_id)
    .bind(format!("/en/content/{target_content_id}"))
    .execute(pool)
    .await
    .unwrap();
    insert_product(pool, target_product_id).await;
    insert_media(pool, asset_id, original_id, display_id).await;

    let extracted = extract_document(&document);
    assert!(extracted.blocking_issues.is_empty());
    assert_eq!(extracted.references.len(), 5);
    assert_eq!(extracted.lock_targets.len(), 3);

    let mut transaction = pool.begin().await.unwrap();
    lock_targets(&mut transaction, &extracted.lock_targets)
        .await
        .unwrap();
    let ready = validate_extracted(&mut transaction, extracted)
        .await
        .unwrap();
    assert!(ready.is_complete(), "issues: {:#?}", ready.blocking_issues);
    assert_eq!(ready.dependencies.len(), 5);
    assert_exact_targets(
        &ready.dependencies,
        target_content_id,
        target_product_id,
        asset_id,
        original_id,
        display_id,
    );
    insert_snapshot(
        &mut transaction,
        source_id,
        1,
        "dependency-contract",
        &ready,
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    let stored_set: (String, i32, Value) = sqlx::query_as(
        r#"SELECT extraction_status,dependency_count,blocking_issues
           FROM cms_publication_dependency_sets
           WHERE content_id=$1 AND content_revision=1"#,
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(stored_set, ("complete".into(), 5, json!([])));
    let stored_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM cms_publication_dependencies WHERE source_content_id=$1",
    )
    .bind(source_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(stored_rows, 5);
    let snapshot_complete: bool =
        sqlx::query_scalar("SELECT cms_publication_dependency_snapshot_complete($1,1)")
            .bind(source_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(snapshot_complete);

    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn zero_dependencies_are_complete_and_wrong_locale_fails_closed() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let pool = sandbox.pool();

    let empty_id = Uuid::new_v4();
    let empty = empty_document("en");
    insert_content(pool, empty_id, "en", "draft", &empty, false).await;
    let mut connection = pool.acquire().await.unwrap();
    let empty_plan = validate_document(&mut connection, &empty).await.unwrap();
    assert!(empty_plan.is_complete());
    assert!(empty_plan.dependencies.is_empty());
    insert_snapshot(
        &mut connection,
        empty_id,
        1,
        "dependency-contract",
        &empty_plan,
    )
    .await
    .unwrap();
    drop(connection);

    let target_id = Uuid::new_v4();
    insert_content(
        pool,
        target_id,
        "zh-CN",
        "published",
        &empty_document("zh-CN"),
        true,
    )
    .await;
    sqlx::query(
        r#"INSERT INTO public_routes
           (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
           VALUES ($1,'content',$2,'zh-CN',$3,false,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(target_id)
    .bind(format!("/zh-CN/content/{target_id}"))
    .execute(pool)
    .await
    .unwrap();
    let mut wrong_locale = empty_document("en");
    wrong_locale["composition"]["blocks"] = json!([{
        "type": "cta",
        "action": {"target": {"targetType": "content", "contentId": target_id}}
    }]);
    let mut connection = pool.acquire().await.unwrap();
    let blocked = validate_document(&mut connection, &wrong_locale)
        .await
        .unwrap();
    assert!(!blocked.is_complete());
    assert!(blocked.blocking_issues.iter().any(|issue| {
        issue.code == DependencyIssueCode::ContentLocaleMismatch
            && issue.path == "/composition/blocks/0/action/target/contentId"
    }));
    assert!(blocked
        .blocking_issues
        .iter()
        .any(|issue| { issue.code == DependencyIssueCode::ContentRouteMissing }));
    drop(connection);

    let zero_set: (String, i32) = sqlx::query_as(
        r#"SELECT extraction_status,dependency_count
           FROM cms_publication_dependency_sets WHERE content_id=$1"#,
    )
    .bind(empty_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(zero_set, ("complete".into(), 0));
    sandbox.cleanup().await;
}

fn dependency_document(
    content_id: Uuid,
    product_id: Uuid,
    asset_id: Uuid,
    _original_id: Uuid,
) -> Value {
    json!({
        "schemaVersion": 2,
        "locale": "en",
        "typeFields": {"type": "page"},
        "seo": {"socialImage": null},
        "composition": {"blocks": [
            {"type": "hero", "media": null, "actions": [{
                "target": {"targetType": "content", "contentId": content_id}
            }]},
            {"type": "media", "media": {
                "asset": {"assetId": asset_id}
            }},
            {"type": "downloadAsset", "asset": {
                "assetId": asset_id
            }}
        ]},
        "relations": [
            {"id": Uuid::new_v4(), "target": {
                "targetType": "content", "contentId": content_id
            }},
            {"id": Uuid::new_v4(), "target": {
                "targetType": "product", "productId": product_id
            }}
        ]
    })
}

fn empty_document(locale: &str) -> Value {
    json!({
        "schemaVersion": 2,
        "locale": locale,
        "typeFields": {"type": "page"},
        "seo": {"socialImage": null},
        "composition": {"blocks": []},
        "relations": []
    })
}

async fn insert_content(
    pool: &PgPool,
    id: Uuid,
    locale: &str,
    status: &str,
    document: &Value,
    published: bool,
) {
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,title,status,is_placeholder,current_revision,
            published_revision,payload,updated_at,data_origin,template_key,
            latest_revision,cms_published_revision,cms_updated_by)
           VALUES ($1,'page',$2,$3,'Dependency fixture',$4,false,1,$5,
                   '{}'::jsonb,now(),'editorial','productIndex',1,NULL,
                   'dependency-contract')"#,
    )
    .bind(id)
    .bind(format!("dependency-{}", id.simple()))
    .bind(locale)
    .bind(status)
    .bind(published.then_some(1_i64))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,
            revision_kind,reason,created_by,created_at)
           VALUES ($1,1,NULL,$2,1,'publish','Dependency contract revision',
                   'dependency-contract',now())"#,
    )
    .bind(id)
    .bind(document)
    .execute(pool)
    .await
    .unwrap();
    if published {
        sqlx::query("UPDATE content_entries SET cms_published_revision=1 WHERE id=$1")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn insert_product(pool: &PgPool, id: Uuid) {
    sqlx::query(
        r#"INSERT INTO products
           (id,stable_id,slug,locale,family,source_snapshot_id,source_revision,
            status,current_revision,published_revision,indexable,payload,updated_at,
            data_origin,product_import_run_id)
           VALUES ($1,$2,$3,'en','axial',NULL,'fixture:dependencies','published',
                   1,1,false,'{}'::jsonb,now(),'developmentFixture',NULL)"#,
    )
    .bind(id)
    .bind(format!("DEV-FIXTURE-{id}"))
    .bind(format!("dependency-product-{}", id.simple()))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO product_revisions
           (product_id,revision,source_snapshot_id,payload,created_at,data_origin,
            product_import_run_id)
           VALUES ($1,1,NULL,'{}'::jsonb,now(),'developmentFixture',NULL)"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_media(pool: &PgPool, asset_id: Uuid, _original_id: Uuid, _display_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,original_name,media_type,byte_size,checksum,metadata,
            created_at,storage_backend,content_type,uploaded_by)
           VALUES ($1,$2,'fixture.png','image/png',16,$3,'{}'::jsonb,
                   now(),'local','image/png','fixture@example.test')"#,
    )
    .bind(asset_id)
    .bind(format!("media/{asset_id}/fixture.png"))
    .bind("a".repeat(64))
    .execute(pool)
    .await
    .unwrap();
}

fn assert_exact_targets(
    dependencies: &[dependencies::PublicationDependencyRow],
    content_id: Uuid,
    product_id: Uuid,
    asset_id: Uuid,
    _original_id: Uuid,
    _display_id: Uuid,
) {
    assert!(dependencies.iter().any(|dependency| matches!(
        dependency.target,
        PublicationDependencyTarget::Content { content_id: id, content_revision: 1 }
            if id == content_id
    )));
    assert!(dependencies.iter().any(|dependency| matches!(
        dependency.target,
        PublicationDependencyTarget::Product { product_id: id, product_revision: 1 }
            if id == product_id
    )));
    assert!(dependencies.iter().any(|dependency| {
        dependency.kind == PublicationDependencyKind::MediaInline
            && matches!(
                dependency.target,
                PublicationDependencyTarget::Media(id) if id == asset_id
            )
    }));
    assert!(dependencies.iter().any(|dependency| {
        dependency.kind == PublicationDependencyKind::MediaDownload
            && matches!(
                dependency.target,
                PublicationDependencyTarget::Media(id) if id == asset_id
            )
    }));
}
