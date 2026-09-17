#[cfg(feature = "devtools")]
use super::*;

#[cfg(feature = "devtools")]
async fn insert_direct_media_asset(pool: &sqlx::PgPool, id: Uuid, name: &str, deleted: bool) {
    sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,public_url,original_name,media_type,byte_size,checksum,metadata,
            created_at,deleted_at,storage_backend,content_type,uploaded_by)
           VALUES ($1,$2,$3,$4,'image/png',1024,$5,'{}'::jsonb,now(),
                   CASE WHEN $6 THEN now() ELSE NULL END,'local','image/png',
                   'uploader@example.test')"#,
    )
    .bind(id)
    .bind(format!("media/{id}/asset.png"))
    .bind(format!("https://media.example.test/media/{id}/asset.png"))
    .bind(name)
    .bind("a".repeat(64))
    .bind(deleted)
    .execute(pool)
    .await
    .expect("insert media asset");
}

#[cfg(feature = "devtools")]
#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn direct_media_listing_is_server_filtered_paginated_and_excludes_deleted() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let app = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state.clone());
    let pool = state.pool.clone();

    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();
    let deleted_id = Uuid::new_v4();
    insert_direct_media_asset(&pool, first_id, "hero-image.png", false).await;
    insert_direct_media_asset(&pool, second_id, "diagram-image.png", false).await;
    insert_direct_media_asset(&pool, deleted_id, "hero-deleted.png", true).await;

    let (status, page) = admin_get_json(&app, "/media/assets?limit=50").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 2);
    let ids = item_ids(&page);
    assert!(ids.contains(&first_id.to_string()));
    assert!(ids.contains(&second_id.to_string()));
    assert!(!ids.contains(&deleted_id.to_string()));
    for item in page["items"].as_array().expect("items") {
        assert_eq!(
            item["publicUrl"],
            format!(
                "https://media.example.test/media/{}/asset.png",
                item["id"].as_str().unwrap()
            )
        );
        assert_eq!(
            item["downloadUrl"],
            format!(
                "/api/public/v1/media/{}/download",
                item["id"].as_str().unwrap()
            )
        );
        assert_eq!(item["sha256"], "a".repeat(64));
        assert!(item.get("scanStatus").is_none());
        assert!(item.get("versionId").is_none());
    }

    let (_, filtered) = admin_get_json(&app, "/media/assets?q=hero-image.png").await;
    assert_eq!(filtered["total"], 1);
    assert_eq!(item_ids(&filtered), vec![first_id.to_string()]);

    let (_, first_page) = admin_get_json(&app, "/media/assets?limit=1").await;
    let cursor = first_page["nextCursor"].as_str().expect("next cursor");
    let (_, second_page) =
        admin_get_json(&app, &format!("/media/assets?limit=1&cursor={cursor}")).await;
    assert_ne!(item_ids(&first_page), item_ids(&second_page));

    let (status, detail) = admin_get_json(&app, &format!("/media/assets/{first_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["id"], first_id.to_string());

    sandbox.cleanup().await;
}

#[cfg(feature = "devtools")]
fn direct_media_state(database_url: &str, root: PathBuf) -> AppState {
    let mut config = Config::for_test();
    config.database_url = Some(database_url.to_owned());
    config.media.storage = Some(airtek_platform::services::media::MediaStorageSettings {
        kind: airtek_platform::services::media::MediaStorageKind::Local,
        local_root: root,
        endpoint: String::new(),
        region: "local".into(),
        bucket: String::new(),
        access_key_id: String::new(),
        secret_access_key: String::new(),
        key_prefix: "media".into(),
        path_style: true,
        public_base_url: "http://localhost/media".into(),
    });
    AppState::new(config).expect("direct media state")
}

#[cfg(feature = "devtools")]
fn media_multipart(boundary: &str, file_name: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[cfg(feature = "devtools")]
async fn upload_direct_media(app: &Router, key: &str, file_name: &str, bytes: &[u8]) -> Response {
    let boundary = format!("airtek-{}", Uuid::new_v4().simple());
    app.clone()
        .oneshot(
            Request::post("/media/assets")
                .header("idempotency-key", key)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(media_multipart(&boundary, file_name, bytes)))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[cfg(feature = "devtools")]
fn stored_files(root: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            if entry.path().is_dir() {
                stored_files(&entry.path())
            } else {
                1
            }
        })
        .sum()
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn direct_upload_is_public_immediately_and_idempotent_per_actor() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let root = std::env::temp_dir().join(format!("airtek-media-{}", Uuid::new_v4()));
    let state = direct_media_state(sandbox.connection_url(), root.clone());
    let principal = cms_principal();
    let principal_email = principal.email.clone();
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(principal))
        .with_state(state.clone());
    let bytes = b"\x89PNG\r\n\x1a\n";
    let key = format!("media-upload-{}", Uuid::new_v4());

    let created = upload_direct_media(&admin, &key, "../hero.png", bytes).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let asset = response_json(created).await;
    assert_eq!(asset["originalName"], "hero.png");
    assert_eq!(asset["mediaType"], "image/png");
    assert_eq!(asset["byteSize"], bytes.len());
    assert_eq!(asset["uploadedBy"], principal_email);
    assert_eq!(asset.as_object().unwrap().len(), 9);
    assert_eq!(stored_files(&root), 1);

    let replay = upload_direct_media(&admin, &key, "../hero.png", bytes).await;
    assert_eq!(replay.status(), StatusCode::CREATED);
    assert_eq!(response_json(replay).await, asset);
    assert_eq!(stored_files(&root), 1);

    let public = airtek_platform::routes::public::router().with_state(state);
    let delivered = public
        .oneshot(
            Request::get(format!("/media/{}", asset["id"].as_str().unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delivered.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        delivered.headers().get(header::LOCATION).unwrap(),
        asset["publicUrl"].as_str().unwrap()
    );

    drop(admin);
    sandbox.cleanup().await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn direct_upload_rejects_key_reuse_for_different_files() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let root = std::env::temp_dir().join(format!("airtek-media-{}", Uuid::new_v4()));
    let state = direct_media_state(sandbox.connection_url(), root.clone());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);
    let key = format!("media-conflict-{}", Uuid::new_v4());
    assert_eq!(
        upload_direct_media(&admin, &key, "first.png", b"\x89PNG\r\n\x1a\n")
            .await
            .status(),
        StatusCode::CREATED
    );
    let conflict = upload_direct_media(&admin, &key, "second.jpg", b"\xff\xd8\xff\xe0").await;
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let problem = response_json(conflict).await;
    assert_eq!(
        problem["type"],
        "https://api.airtekpower.example/problems/media_idempotency_conflict"
    );
    assert_eq!(stored_files(&root), 1);

    drop(admin);
    sandbox.cleanup().await;
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn failed_catalogue_write_compensates_the_uploaded_object() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    sqlx::query(
        "ALTER TABLE audit_log ADD CONSTRAINT reject_media_upload_audit CHECK (action <> 'media.upload') NOT VALID",
    )
    .execute(sandbox.pool())
    .await
    .unwrap();
    let root = std::env::temp_dir().join(format!("airtek-media-{}", Uuid::new_v4()));
    let state = direct_media_state(sandbox.connection_url(), root.clone());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let failed = upload_direct_media(
        &admin,
        &format!("media-db-failure-{}", Uuid::new_v4()),
        "rollback.png",
        b"\x89PNG\r\n\x1a\n",
    )
    .await;
    assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(stored_files(&root), 0);
    let assets: i64 = sqlx::query_scalar("SELECT count(*) FROM media_assets")
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(assets, 0);

    drop(admin);
    sandbox.cleanup().await;
    if root.exists() {
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn unavailable_storage_fails_without_catalogue_rows() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let root = std::env::temp_dir().join(format!("airtek-media-file-{}", Uuid::new_v4()));
    std::fs::write(&root, b"not a directory").unwrap();
    let state = direct_media_state(sandbox.connection_url(), root.clone());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let failed = upload_direct_media(
        &admin,
        &format!("media-storage-failure-{}", Uuid::new_v4()),
        "unavailable.png",
        b"\x89PNG\r\n\x1a\n",
    )
    .await;
    assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
    let assets: i64 = sqlx::query_scalar("SELECT count(*) FROM media_assets")
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(assets, 0);

    drop(admin);
    sandbox.cleanup().await;
    std::fs::remove_file(root).unwrap();
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn upload_without_database_object_storage_settings_returns_503() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let admin = airtek_platform::routes::admin::router()
        .layer(Extension(cms_principal()))
        .with_state(state);

    let response = upload_direct_media(
        &admin,
        &format!("media-unconfigured-{}", Uuid::new_v4()),
        "unconfigured.png",
        b"\x89PNG\r\n\x1a\n",
    )
    .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let assets: i64 = sqlx::query_scalar("SELECT count(*) FROM media_assets")
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(assets, 0);

    drop(admin);
    sandbox.cleanup().await;
}
