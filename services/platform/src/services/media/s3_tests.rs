use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::mpsc,
    thread,
    time::Duration,
};

use chrono::{TimeZone, Utc};
use http_body_util::BodyExt;

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

fn settings_at(endpoint: String) -> MediaStorageSettings {
    let mut settings = settings(true);
    settings.endpoint = endpoint;
    settings
}

fn read_request_head(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set request timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while !request.windows(4).any(|part| part == b"\r\n\r\n") {
        let read = stream.read(&mut buffer).expect("read request");
        assert!(read > 0, "request ended before its headers");
        request.extend_from_slice(&buffer[..read]);
    }
    assert!(request.starts_with(b"GET "));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s3_get_returns_after_headers_and_streams_the_later_body() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture S3 server");
    let endpoint = format!("http://{}", listener.local_addr().expect("fixture address"));
    let expected = vec![0x3c; 3 * 64 * 1024 + 19];
    let served = expected.clone();
    let (release_sender, release_receiver) = mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fixture request");
        read_request_head(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            served.len()
        )
        .expect("write response head");
        stream.flush().expect("flush response head");
        release_receiver
            .recv_timeout(Duration::from_secs(3))
            .expect("client accepted response head");
        for chunk in served.chunks(32 * 1024) {
            stream.write_all(chunk).expect("write response body chunk");
        }
    });

    let object = tokio::time::timeout(
        Duration::from_secs(1),
        get(
            &settings_at(endpoint),
            "media/2026/09/stream.png",
            expected.len() as u64,
        ),
    )
    .await
    .expect("GET returns before the upstream finishes its body")
    .expect("open S3 stream");
    release_sender.send(()).expect("release fixture body");
    let received = object
        .body
        .collect()
        .await
        .expect("collect streamed body")
        .to_bytes();

    assert_eq!(object.content_length, expected.len() as u64);
    assert_eq!(received.as_ref(), expected.as_slice());
    server.join().expect("fixture S3 server completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s3_get_decodes_chunked_upstream_without_buffering_the_object() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture S3 server");
    let endpoint = format!("http://{}", listener.local_addr().expect("fixture address"));
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fixture request");
        read_request_head(&mut stream);
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                  5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
            )
            .expect("write chunked fixture response");
    });

    let object = get(&settings_at(endpoint), "media/2026/09/chunked.png", 11)
        .await
        .expect("open chunked S3 stream");
    let received = object
        .body
        .collect()
        .await
        .expect("collect decoded body")
        .to_bytes();

    assert_eq!(received.as_ref(), b"hello world");
    server.join().expect("fixture S3 server completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s3_get_surfaces_an_upstream_body_that_ends_early() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture S3 server");
    let endpoint = format!("http://{}", listener.local_addr().expect("fixture address"));
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fixture request");
        read_request_head(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhi")
            .expect("write truncated fixture response");
    });

    let object = get(&settings_at(endpoint), "media/2026/09/truncated.png", 5)
        .await
        .expect("response head is valid");
    let error = object
        .body
        .collect()
        .await
        .expect_err("truncated upstream body must fail the client stream");

    assert!(error.to_string().contains("ended early"));
    server.join().expect("fixture S3 server completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s3_get_maps_a_missing_object_before_starting_the_body() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture S3 server");
    let endpoint = format!("http://{}", listener.local_addr().expect("fixture address"));
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept fixture request");
        read_request_head(&mut stream);
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
            .expect("write missing fixture response");
    });

    let error = match get(&settings_at(endpoint), "media/2026/09/missing.png", 5).await {
        Ok(_) => panic!("missing S3 object must fail before body delivery"),
        Err(error) => error,
    };

    assert_eq!(error.status(), axum::http::StatusCode::NOT_FOUND);
    server.join().expect("fixture S3 server completed");
}
