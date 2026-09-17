use std::path::PathBuf;

use futures_util::StreamExt;

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
async fn local_object_is_read_as_bounded_async_chunks() {
    let root = std::env::temp_dir().join(format!("airtek-media-stream-{}", Uuid::new_v4()));
    let settings = local_settings(root.clone());
    let key = "media/2026/09/stream.png";
    let expected = vec![0x5a; 2 * 64 * 1024 + 17];

    put_object(&settings, key, "image/png", expected.clone())
        .await
        .expect("write fixture object");
    let object = get_object(&settings, key, expected.len() as u64)
        .await
        .expect("open fixture stream");
    assert_eq!(object.content_length, expected.len() as u64);

    let mut chunks = object.body.into_data_stream();
    let mut chunk_count = 0;
    let mut received = Vec::new();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.expect("read streamed chunk");
        chunk_count += 1;
        received.extend_from_slice(&chunk);
    }
    assert!(chunk_count >= 3);
    assert_eq!(received, expected);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn local_object_rejects_catalogue_length_mismatch_before_streaming() {
    let root = std::env::temp_dir().join(format!("airtek-media-length-{}", Uuid::new_v4()));
    let settings = local_settings(root.clone());
    let key = "media/2026/09/mismatch.png";

    put_object(&settings, key, "image/png", vec![1, 2, 3])
        .await
        .expect("write fixture object");
    let error = match get_object(&settings, key, 4).await {
        Ok(_) => panic!("catalogue mismatch must fail"),
        Err(error) => error,
    };
    assert_eq!(error.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);

    let _ = std::fs::remove_dir_all(root);
}
