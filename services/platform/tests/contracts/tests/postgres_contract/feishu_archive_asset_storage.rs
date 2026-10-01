use airtek_runtime::services::feishu::{store_archive_asset, SourceAttachment};
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use image::{ImageFormat, Rgba, RgbaImage};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use tower::ServiceExt;

use super::*;

struct ArchiveFixture {
    name: &'static str,
    mime: &'static str,
    usage: &'static str,
    bytes: Vec<u8>,
}

fn fixtures() -> Vec<ArchiveFixture> {
    let mut png = Vec::new();
    RgbaImage::from_pixel(1, 1, Rgba([0x1f, 0x7a, 0x5c, 0xff]))
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .unwrap();
    vec![
        ArchiveFixture {
            name: "drawing.png",
            mime: "image/png",
            usage: "drawing",
            bytes: png,
        },
        ArchiveFixture {
            name: "catalog.pdf",
            mime: "application/pdf",
            usage: "datasheet",
            bytes: b"%PDF-1.7\nAIRTEK archive PDF".to_vec(),
        },
        ArchiveFixture {
            name: "specification.docx",
            mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            usage: "datasheet",
            bytes: b"PK\x03\x04word/document.xml".to_vec(),
        },
        ArchiveFixture {
            name: "curve.xlsx",
            mime: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            usage: "curve",
            bytes: b"PK\x03\x04xl/workbook.xml".to_vec(),
        },
        ArchiveFixture {
            name: "drawing.dwg",
            mime: "image/vnd.dwg",
            usage: "cad",
            bytes: b"AC1027AIRTEK DWG".to_vec(),
        },
        ArchiveFixture {
            name: "model.step",
            mime: "model/step",
            usage: "cad",
            bytes: b"ISO-10303-21;\nHEADER;".to_vec(),
        },
    ]
}

async fn write_fixture(root: &std::path::Path, fixture: &ArchiveFixture) -> std::path::PathBuf {
    let path = root.join(fixture.name);
    tokio::fs::write(&path, &fixture.bytes).await.unwrap();
    path
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL and AIRTEK_TEST_S3_ENDPOINT MinIO"]
async fn archive_asset_types_copy_and_download_with_public_mime_validation() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let endpoint = std::env::var("AIRTEK_TEST_S3_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:19000".into());
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let suffix = Uuid::new_v4().simple().to_string();
    let prefix = format!("media/archive-assets-{suffix}");
    let public_base = format!("{endpoint}/airtek-media");
    sqlx::query(
        r#"INSERT INTO object_storage_settings
           (singleton,provider,endpoint,region,bucket,access_key_id,secret_access_key,
            key_prefix,path_style,public_base_url,updated_by)
           VALUES (true,'s3',$1,'us-east-1','airtek-media','airtek-media-api',
                   'local-api-media-only',$2,true,$3,'archive-contract')"#,
    )
    .bind(&endpoint)
    .bind(&prefix)
    .bind(&public_base)
    .execute(sandbox.pool())
    .await
    .unwrap();

    let mut config = Config::for_test();
    config.database_url = Some(sandbox.connection_url().into());
    let state = AppState::new(config).unwrap();
    let connector_id: Uuid =
        sqlx::query_scalar("SELECT connector_id FROM feishu_connector_settings LIMIT 1")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    let sync_run_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,connector_id,source,dry_run,mapping_version,status,started_at,payload)
           VALUES ($1,$2,'feishu',false,'archive-assets','validating',now(),'{}')"#,
    )
    .bind(sync_run_id)
    .bind(connector_id)
    .execute(&state.pool)
    .await
    .unwrap();

    let root = std::env::temp_dir().join(format!("airtek-archive-assets-{suffix}"));
    tokio::fs::create_dir_all(&root).await.unwrap();
    let app = build_router(state.clone());
    for (index, fixture) in fixtures().iter().enumerate() {
        let path = write_fixture(&root, fixture).await;
        let checksum = format!("{:x}", Sha256::digest(&fixture.bytes));
        let attachment = SourceAttachment {
            file_token: format!("archive-token-{suffix}-{index}"),
            table_id: "tblJjxOgBFL0FD0N".into(),
            source_record_id: format!("rec-{suffix}-{index}"),
            original_name: fixture.name.into(),
            declared_size: Some(fixture.bytes.len() as u64),
            declared_media_type: Some(fixture.mime.into()),
            source_field_id: format!("fld-{index}"),
            source_field_name: fixture.name.into(),
            usage: fixture.usage.into(),
            source_revision: format!("archive-{suffix}"),
        };
        let stored = store_archive_asset(
            &state,
            connector_id,
            sync_run_id,
            &attachment,
            &path,
            &checksum,
            fixture.bytes.len() as i64,
        )
        .await
        .unwrap();
        assert!(stored.newly_created);
        assert_eq!(stored.media_type, fixture.mime);
        assert_eq!(stored.checksum, checksum);
        let raster = matches!(fixture.mime, "image/png" | "image/jpeg" | "image/webp");
        assert_eq!(stored.preview_storage_key.is_some(), raster);

        let download = app
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/public/v1/media/{}/download",
                    stored.media_asset_id
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(download.status(), StatusCode::PERMANENT_REDIRECT);
        let location = download.headers()[header::LOCATION].to_str().unwrap();
        let public = reqwest::get(location).await.unwrap();
        assert_eq!(public.status(), StatusCode::OK);
        assert_eq!(public.bytes().await.unwrap().as_ref(), fixture.bytes);

        if let Some(preview_key) = &stored.preview_storage_key {
            let preview = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/public/v1/media/{}", stored.media_asset_id))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(preview.status(), StatusCode::PERMANENT_REDIRECT);
            let preview_url = format!("{public_base}/{preview_key}");
            assert_eq!(
                reqwest::get(preview_url).await.unwrap().status(),
                StatusCode::OK
            );
        }
    }

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM media_assets")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(count, fixtures().len() as i64);
    let metadata: Vec<(String, i64)> =
        sqlx::query_as("SELECT media_type,byte_size FROM media_assets ORDER BY original_name")
            .fetch_all(&state.pool)
            .await
            .unwrap();
    assert_eq!(metadata.len(), fixtures().len());
    assert!(metadata.iter().all(|(_, byte_size)| *byte_size > 0));

    tokio::fs::remove_dir_all(&root).await.unwrap();
    state.pool.close().await;
    sandbox.cleanup().await;
}
