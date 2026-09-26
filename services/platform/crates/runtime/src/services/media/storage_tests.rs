use std::path::PathBuf;

use sha2::Digest;

use super::*;
use crate::services::media::config::MediaStorageKind;

fn local_settings(root: PathBuf) -> MediaStorageSettings {
    MediaStorageSettings {
        kind: MediaStorageKind::Local,
        local_root: root,
        endpoint: String::new(),
        region: "us-east-1".to_owned(),
        bucket: String::new(),
        access_key_id: String::new(),
        secret_access_key: String::new(),
        key_prefix: "media".to_owned(),
        path_style: true,
        public_base_url: "http://localhost/media".to_owned(),
    }
}

#[tokio::test]
async fn local_object_delete_is_idempotent() {
    let root = std::env::temp_dir().join(format!("airtek-media-delete-{}", Uuid::new_v4()));
    let settings = local_settings(root.clone());
    let key = "media/2026/09/object.png";

    put_object(&settings, key, "image/png", vec![1, 2, 3])
        .await
        .expect("write fixture object");
    assert!(local_path(&settings, key).unwrap().is_file());

    delete_object(&settings, key)
        .await
        .expect("delete existing object");
    delete_object(&settings, key)
        .await
        .expect("repeated delete is successful");
    assert!(!local_path(&settings, key).unwrap().exists());

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn local_file_upload_streams_from_staging_without_consuming_it() {
    let root = std::env::temp_dir().join(format!("airtek-media-file-{}", Uuid::new_v4()));
    let source = std::env::temp_dir().join(format!("airtek-media-source-{}", Uuid::new_v4()));
    let settings = local_settings(root.clone());
    let key = "media/2026/09/document.pdf";
    let expected = vec![0x5a; 3 * 64 * 1024 + 17];
    std::fs::write(&source, &expected).expect("write staged fixture");
    let checksum = format!("{:x}", sha2::Sha256::digest(&expected));

    put_file(
        &settings,
        key,
        "application/pdf",
        &source,
        expected.len() as u64,
        &checksum,
    )
    .await
    .expect("stream staged file");

    assert_eq!(
        std::fs::read(local_path(&settings, key).unwrap()).unwrap(),
        expected
    );
    assert!(source.is_file());
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_dir_all(root);
}
