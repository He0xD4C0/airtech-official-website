#[cfg(feature = "devtools")]
fn expected_media_version_id(asset_id: Uuid) -> Uuid {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"airtek-cms-v2-media-version\0");
    digest.update(asset_id.as_bytes());
    let hash = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[cfg(feature = "devtools")]
async fn insert_media_asset(
    pool: &sqlx::PgPool,
    id: Uuid,
    name: &str,
    scan_status: &str,
    access_level: &str,
    deleted: bool,
) {
    sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,original_name,media_type,byte_size,checksum,
            scan_status,access_level,metadata,created_at,deleted_at)
           VALUES ($1,$2,$3,'image/png',1024,'checksum',$4,$5,'{}'::jsonb,now(),
                   CASE WHEN $6 THEN now() ELSE NULL END)"#,
    )
    .bind(id)
    .bind(format!("fixtures/{id}"))
    .bind(name)
    .bind(scan_status)
    .bind(access_level)
    .bind(deleted)
    .execute(pool)
    .await
    .expect("insert media asset");
}

#[cfg(feature = "devtools")]
#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn media_asset_listing_derives_versions_filters_and_excludes_deleted() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_version_range(1, 11).await;
    let state = postgres_state(sandbox.connection_url());
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state.clone());
    let pool = state.pool.clone().expect("PostgreSQL pool");

    let clean_id = Uuid::new_v4();
    let internal_id = Uuid::new_v4();
    let deleted_id = Uuid::new_v4();
    insert_media_asset(&pool, clean_id, "hero-image.png", "clean", "public", false).await;
    insert_media_asset(&pool, internal_id, "internal-draft.png", "pending", "internal", false).await;
    insert_media_asset(&pool, deleted_id, "hero-image-deleted.png", "clean", "public", true).await;

    let (status, page) = admin_get_json(&app, "/media/assets?limit=50").await;
    assert_eq!(status, StatusCode::OK);
    let ids = item_ids(&page);
    assert!(ids.contains(&clean_id.to_string()));
    assert!(ids.contains(&internal_id.to_string()));
    assert!(
        !ids.contains(&deleted_id.to_string()),
        "soft-deleted assets must never be listed"
    );
    for item in page["items"].as_array().expect("items array") {
        let id: Uuid = item["id"].as_str().unwrap().parse().unwrap();
        let version_id: Uuid = item["versionId"].as_str().unwrap().parse().unwrap();
        assert_eq!(
            version_id,
            expected_media_version_id(id),
            "versionId must equal stable_media_version_id"
        );
    }

    let (_, by_name) = admin_get_json(&app, "/media/assets?q=hero-image.png").await;
    assert_eq!(item_ids(&by_name), vec![clean_id.to_string()]);

    let (_, by_status) = admin_get_json(&app, "/media/assets?scanStatus=clean").await;
    let status_ids = item_ids(&by_status);
    assert!(status_ids.contains(&clean_id.to_string()));
    assert!(!status_ids.contains(&internal_id.to_string()));

    let (_, by_access) = admin_get_json(&app, "/media/assets?accessLevel=internal").await;
    assert_eq!(item_ids(&by_access), vec![internal_id.to_string()]);

    let (_, first_page) = admin_get_json(&app, "/media/assets?limit=1").await;
    assert_eq!(first_page["items"].as_array().unwrap().len(), 1);
    let cursor = first_page["nextCursor"].as_str().expect("next cursor");
    let (_, second_page) = admin_get_json(
        &app,
        &format!("/media/assets?limit=1&cursor={cursor}"),
    )
    .await;
    assert_eq!(second_page["items"].as_array().unwrap().len(), 1);
    assert_ne!(
        item_ids(&first_page),
        item_ids(&second_page),
        "cursor pagination must not repeat rows"
    );

    let (invalid, _) = admin_get_json(&app, "/media/assets?scanStatus=unknown").await;
    assert_eq!(invalid, StatusCode::BAD_REQUEST);

    sandbox.cleanup().await;
}
