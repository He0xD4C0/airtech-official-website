use super::*;

fn multipart(content_type: &str, body: &[u8]) -> ParsedUpload {
    parse_upload_body(Some(content_type), body).expect("multipart body parses")
}

fn png() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    bytes.extend_from_slice(&[0x00, 0x01, 0x02, 0x03]);
    bytes
}

#[test]
fn sniffs_only_the_supported_raster_formats() {
    assert_eq!(
        sniff_media_type(&png()).map(|value| value.mime),
        Some("image/png")
    );
    assert_eq!(
        sniff_media_type(&[0xff, 0xd8, 0xff, 0xe0]).map(|value| value.mime),
        Some("image/jpeg")
    );
    let mut webp = b"RIFF".to_vec();
    webp.extend_from_slice(&[0x1a, 0x00, 0x00, 0x00]);
    webp.extend_from_slice(b"WEBP");
    assert_eq!(
        sniff_media_type(&webp).map(|value| value.mime),
        Some("image/webp")
    );
    assert_eq!(
        sniff_media_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        None
    );
    assert_eq!(sniff_media_type(b""), None);
}

#[test]
fn parses_a_single_file_part_and_sanitizes_its_name() {
    let mut request = b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"../nested/hero.PNG\"\r\nContent-Type: image/png\r\n\r\n".to_vec();
    request.extend_from_slice(&png());
    request.extend_from_slice(b"\r\n--boundary--\r\n");
    let parsed = multipart("multipart/form-data; boundary=boundary", &request);
    assert_eq!(parsed.file_name, "hero.PNG");
    assert_eq!(parsed.bytes, png());
}

#[test]
fn ignores_unrelated_parts_and_requires_a_file_part() {
    let mut request =
        b"--b\r\nContent-Disposition: form-data; name=\"caption\"\r\n\r\nhello\r\n".to_vec();
    request.extend_from_slice(
        b"--b\r\nContent-Disposition: form-data; name=\"note\"; filename=\"a.png\"\r\n\r\n",
    );
    request.extend_from_slice(b"\r\n--b--\r\n");
    assert!(parse_upload_body(Some("multipart/form-data; boundary=b"), &request).is_err());
}

#[test]
fn rejects_non_multipart_bodies() {
    assert!(parse_upload_body(Some("image/png"), &png()).is_err());
    assert!(parse_upload_body(None, &png()).is_err());
    assert!(parse_upload_body(Some("multipart/form-data"), b"--b\r\n\r\n--b--\r\n").is_err());
}

#[test]
fn accepts_an_empty_file_part_so_the_sniffer_reports_unsupported_type() {
    let mut request =
        b"--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"empty.bin\"\r\n\r\n"
            .to_vec();
    request.extend_from_slice(b"\r\n--b--\r\n");
    let parsed = multipart("multipart/form-data; boundary=b", &request);
    assert!(parsed.bytes.is_empty());
    assert_eq!(sniff_media_type(&parsed.bytes), None);
}

#[test]
fn applies_the_25_mib_limit_to_the_file_not_multipart_overhead() {
    assert!(ensure_upload_file_size(&vec![0_u8; MAX_MEDIA_UPLOAD_BYTES]).is_ok());
    assert!(ensure_upload_file_size(&vec![0_u8; MAX_MEDIA_UPLOAD_BYTES + 1]).is_err());
}

#[test]
fn every_human_review_decision_requires_a_non_empty_reason() {
    for status in ["clean", "quarantined"] {
        assert!(human_review_decision(status, None).is_err());
        assert!(human_review_decision(status, Some(" \n\t ")).is_err());
        let decision = human_review_decision(status, Some("  Human review evidence  "))
            .expect("non-empty review reason");
        assert_eq!(decision.0, status);
        assert_eq!(decision.1, "Human review evidence");
    }
}

#[test]
fn human_review_decisions_reject_unknown_status_and_oversized_reasons() {
    assert!(human_review_decision("pending", Some("Human review evidence")).is_err());
    assert!(human_review_decision("clean", Some(&"r".repeat(501))).is_err());
}

fn contains_file(path: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        path.is_file() || (path.is_dir() && contains_file(&path))
    })
}

#[tokio::test]
async fn database_failure_after_put_deletes_the_local_object_and_preserves_the_error() {
    let root = std::env::temp_dir().join(format!("airtek-media-compensation-{}", Uuid::new_v4()));
    let mut config = crate::Config::for_test();
    config.media.storage = Some(crate::services::media::MediaStorageSettings {
        kind: crate::services::media::MediaStorageKind::Local,
        local_root: root.clone(),
        endpoint: String::new(),
        region: "us-east-1".to_owned(),
        bucket: String::new(),
        access_key_id: String::new(),
        secret_access_key: String::new(),
        key_prefix: "media".to_owned(),
        path_style: true,
        public_base_url: "http://localhost/media".to_owned(),
    });
    let state = AppState::new(config).expect("test state");
    let mut request =
        b"--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"fixture.png\"\r\nContent-Type: image/png\r\n\r\n"
            .to_vec();
    request.extend_from_slice(&png());
    request.extend_from_slice(b"\r\n--b--\r\n");

    let error = upload_media_asset(
        &state,
        "media-test@example.com",
        Uuid::new_v4(),
        Some("multipart/form-data; boundary=b"),
        &request,
    )
    .await
    .expect_err("missing PostgreSQL must fail after object PUT");

    assert_eq!(error.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
    assert!(root.join("media").is_dir(), "the object write phase ran");
    assert!(
        !contains_file(&root),
        "compensation removed the stored object"
    );
    let _ = std::fs::remove_dir_all(root);
}
