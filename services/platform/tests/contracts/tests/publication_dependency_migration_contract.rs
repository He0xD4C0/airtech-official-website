use serde_json::json;
use uuid::Uuid;

mod support;

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn v0013_and_v0014_define_direct_media_and_immutable_dependencies() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 14).await;
    let pool = sandbox.pool();

    let content_id = Uuid::new_v4();
    let asset_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,title,status,is_placeholder,current_revision,
            published_revision,payload,updated_at,data_origin,template_key,
            latest_revision,cms_published_revision,cms_updated_by)
           VALUES ($1,'page',$2,'en','Dependency fixture','draft',false,1,NULL,
                   '{}'::jsonb,now(),'editorial','productIndex',1,NULL,'contract')"#,
    )
    .bind(content_id)
    .bind(format!("dependency-{}", content_id.simple()))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,revision_kind,
            reason,created_by,created_at)
           VALUES ($1,1,NULL,$2,1,'manual','Contract revision','contract',now())"#,
    )
    .bind(content_id)
    .bind(json!({"schemaVersion": 2, "locale": "en"}))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,original_name,media_type,byte_size,checksum,metadata,
            created_at,storage_backend,content_type,uploaded_by)
           VALUES ($1,$2,'direct.png','image/png',8,$3,'{}'::jsonb,now(),
                   'local','image/png','contract@example.test')"#,
    )
    .bind(asset_id)
    .bind(format!("media/{asset_id}/direct.png"))
    .bind("a".repeat(64))
    .execute(pool)
    .await
    .unwrap();

    let compatibility: (String, String) =
        sqlx::query_as("SELECT scan_status,access_level FROM media_assets WHERE id=$1")
            .bind(asset_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(compatibility, ("clean".into(), "public".into()));

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(
        r#"INSERT INTO cms_publication_dependency_sets
           (content_id,content_revision,extractor_version,extraction_status,
            dependency_count,blocking_issues,created_by)
           VALUES ($1,1,'direct-v1','complete',1,'[]'::jsonb,'contract')"#,
    )
    .bind(content_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO cms_publication_dependencies
           (id,source_content_id,source_revision,reference_path,dependency_kind,
            target_media_asset_id)
           VALUES ($1,$2,1,'/composition/blocks/0/media/asset/assetId',
                   'mediaInline',$3)"#,
    )
    .bind(Uuid::new_v4())
    .bind(content_id)
    .bind(asset_id)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    let complete: bool =
        sqlx::query_scalar("SELECT cms_publication_dependency_snapshot_complete($1,1)")
            .bind(content_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(complete);
    let reverse_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM cms_publication_dependencies WHERE target_media_asset_id=$1",
    )
    .bind(asset_id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(reverse_count, 1);

    let immutable = sqlx::query(
        "UPDATE cms_publication_dependencies SET reference_path='/changed' WHERE target_media_asset_id=$1",
    )
    .bind(asset_id)
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(
        immutable
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("55000")
    );

    sandbox.cleanup().await;
}
