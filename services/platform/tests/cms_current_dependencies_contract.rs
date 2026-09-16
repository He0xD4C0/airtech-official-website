use serde_json::{json, Value};
use uuid::Uuid;

use airtek_platform::services::cms_publication_dependencies::{
    extract_document, lock_targets, replace_current, validate_extracted,
    PublicationDependencyTarget,
};

mod support;

fn dependency_document(id: Uuid, target: Option<Uuid>) -> Value {
    json!({
        "schemaVersion": 2,
        "kind": "news",
        "locale": "en",
        "templateKey": "newsDetail",
        "title": "TEST ONLY dependency",
        "slug": format!("dependency-{id}"),
        "summary": null,
        "isPlaceholder": true,
        "typeFields": {
            "type": "news", "category": "Company", "authorDisplayName": "AIRTEKPOWER",
            "publicationAt": null, "cover": null, "featured": false
        },
        "body": {"type": "doc", "content": [{"type": "paragraph"}]},
        "composition": {"blocks": []},
        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
        "relations": target.map(|target_id| vec![json!({
            "id": Uuid::new_v4(), "slot": "related",
            "target": {"targetType": "content", "contentId": target_id}
        })]).unwrap_or_default(),
        "draftVersion": 1
    })
}

async fn insert_current_publication(pool: &sqlx::PgPool, id: Uuid, document: &Value) {
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,is_placeholder,data_origin,template_key,created_at)
           VALUES ($1,'news',$2,'en',true,'editorial','newsDetail',now())"#,
    )
    .bind(id)
    .bind(document["slug"].as_str().unwrap())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO cms_published_content
           (content_id,document,publication_version,published_at,updated_at)
           VALUES ($1,$2,1,now(),now())"#,
    )
    .bind(id)
    .bind(document)
    .execute(pool)
    .await
    .unwrap();
}

#[test]
fn extraction_uses_current_content_identity_without_revision_target() {
    let target = Uuid::new_v4();
    let extracted = extract_document(&dependency_document(Uuid::new_v4(), Some(target)));
    assert!(extracted.blocking_issues.is_empty());
    assert_eq!(extracted.references.len(), 1);
    assert_eq!(extracted.lock_targets.len(), 1);
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn current_dependencies_replace_the_previous_set() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let source = Uuid::new_v4();
    let target = Uuid::new_v4();
    let target_document = dependency_document(target, None);
    let source_document = dependency_document(source, Some(target));
    insert_current_publication(sandbox.pool(), target, &target_document).await;
    insert_current_publication(sandbox.pool(), source, &source_document).await;
    sqlx::query(
        r#"INSERT INTO public_routes
           (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
           VALUES ($1,'content',$2,'en',$3,false,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(target)
    .bind(format!("/en/news/{}", target.simple()))
    .execute(sandbox.pool())
    .await
    .unwrap();

    let extracted = extract_document(&source_document);
    let mut transaction = sandbox.pool().begin().await.unwrap();
    lock_targets(&mut transaction, &extracted.lock_targets)
        .await
        .unwrap();
    let plan = validate_extracted(&mut transaction, extracted)
        .await
        .unwrap();
    assert!(plan.is_complete(), "issues: {:#?}", plan.blocking_issues);
    assert!(plan.dependencies.iter().any(|dependency| {
        matches!(dependency.target, PublicationDependencyTarget::Content(id) if id == target)
    }));
    replace_current(&mut transaction, source, 1, "contract", &plan)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let stored: Option<Uuid> = sqlx::query_scalar(
        "SELECT target_content_id FROM cms_current_publication_dependencies WHERE source_content_id=$1",
    )
    .bind(source)
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(stored, Some(target));

    let empty = extract_document(&dependency_document(source, None));
    let mut transaction = sandbox.pool().begin().await.unwrap();
    let empty_plan = validate_extracted(&mut transaction, empty).await.unwrap();
    replace_current(&mut transaction, source, 2, "contract", &empty_plan)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM cms_current_publication_dependencies WHERE source_content_id=$1",
    )
    .bind(source)
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(count, 0);
    sandbox.cleanup().await;
}
