use std::path::PathBuf;

use chrono::{TimeZone, Utc};

use super::*;
use crate::services::media::config::MediaStorageKind;

fn settings(path_style: bool) -> MediaStorageSettings {
    MediaStorageSettings {
        kind: MediaStorageKind::S3,
        local_root: PathBuf::new(),
        endpoint: "http://minio.test:9000/base".to_owned(),
        region: "us-east-1".to_owned(),
        bucket: "airtek-media".to_owned(),
        access_key_id: "AKIAIOSFODNN7EXAMPLE".to_owned(),
        secret_access_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".to_owned(),
        key_prefix: "media".to_owned(),
        path_style,
        public_base_url: "http://media.test/airtek-media".to_owned(),
    }
}

#[test]
fn derives_the_complete_aws4_signing_key() {
    let key = signing_key(
        "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        "20130524",
        "us-east-1",
        "s3",
    )
    .expect("signing key");
    assert_eq!(
        HEXLOWER.encode(&key),
        "f117494eff5d09da21cbf7f0339559ea04fc9582d31299cb992be70a6b27c97a"
    );
}

#[test]
fn signs_a_fixed_s3_request_with_the_expected_signature() {
    let now = Utc
        .with_ymd_and_hms(2013, 5, 24, 0, 0, 0)
        .single()
        .expect("fixed timestamp");
    let headers = authorize_at(
        &settings(true),
        "GET",
        "/examplebucket/test.txt",
        "examplebucket.s3.amazonaws.com",
        &[],
        &[],
        now,
    )
    .expect("signed headers");
    let authorization = headers
        .iter()
        .find(|(name, _)| name == "authorization")
        .map(|(_, value)| value.as_str())
        .expect("authorization header");
    assert_eq!(
        authorization,
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature=e262308ffddff051393fc1e4f15361c623b2f8157cc41c3a1441cdba342e744d"
    );
}

#[test]
fn object_path_uses_the_persisted_prefix_exactly_once() {
    let endpoint = parse_endpoint("http://minio.test:9000/base").expect("endpoint");
    let key = "media/2026/09/asset.png";
    assert_eq!(
        object_path(&settings(true), &endpoint, key),
        "/base/airtek-media/media/2026/09/asset.png"
    );
    assert_eq!(
        object_path(&settings(false), &endpoint, key),
        "/base/media/2026/09/asset.png"
    );
}

#[test]
fn builds_a_signed_empty_delete_request_for_the_exact_object_path() {
    let now = Utc
        .with_ymd_and_hms(2013, 5, 24, 0, 0, 0)
        .single()
        .expect("fixed timestamp");
    let settings = settings(true);
    let endpoint = parse_endpoint(&settings.endpoint).expect("endpoint");
    let path = object_path(&settings, &endpoint, "media/2026/09/asset.png");
    let headers = authorize_at(
        &settings,
        "DELETE",
        &path,
        &endpoint.host_header(),
        &[],
        &[],
        now,
    )
    .expect("signed delete headers");
    let request = String::from_utf8(render_request("DELETE", &path, &headers, &[]))
        .expect("HTTP request is ASCII");

    assert!(request.starts_with("DELETE /base/airtek-media/media/2026/09/asset.png HTTP/1.1\r\n"));
    assert!(request.contains("authorization: AWS4-HMAC-SHA256 Credential="));
    assert!(request.contains(
        "x-amz-content-sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\r\n"
    ));
    assert!(request.contains("content-length: 0\r\n"));
    assert!(request.ends_with("\r\n\r\n"));
}

#[test]
fn delete_treats_missing_objects_as_success() {
    for status in [200, 204, 404] {
        assert!(accept_delete_status(status).is_ok(), "status {status}");
    }
    assert!(accept_delete_status(403).is_err());
}
