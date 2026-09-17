use std::{
    io::{ErrorKind, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::{Arc, OnceLock},
    time::Duration,
};

use chrono::{DateTime, Utc};
use data_encoding::HEXLOWER;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::error::ApiError;

use super::config::MediaStorageSettings;
use super::storage::MediaObject;

type HmacSha256 = Hmac<Sha256>;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(60);
const HEADER_LIMIT: usize = 64 * 1024;

#[allow(dead_code)]
#[path = "s3_stream.rs"]
mod get_stream;

pub(super) fn put(
    settings: &MediaStorageSettings,
    key: &str,
    content_type: &str,
    bytes: &[u8],
) -> Result<(), ApiError> {
    let response = exchange(
        settings,
        "PUT",
        key,
        vec![("content-type".to_owned(), content_type.to_owned())],
        bytes,
        HEADER_LIMIT,
    )?;
    match response.status {
        200 => Ok(()),
        status => Err(storage_failure("Media object could not be stored.", status)),
    }
}

#[allow(dead_code)]
pub(super) async fn get(
    settings: &MediaStorageSettings,
    key: &str,
    expected_size: u64,
) -> Result<MediaObject, ApiError> {
    get_stream::get(settings, key, expected_size).await
}

pub(super) fn delete(settings: &MediaStorageSettings, key: &str) -> Result<(), ApiError> {
    let response = exchange(settings, "DELETE", key, Vec::new(), &[], HEADER_LIMIT)?;
    accept_delete_status(response.status)
}

pub(super) fn probe_public_url(url: &str) -> Result<(), ApiError> {
    let endpoint = parse_endpoint(url)?;
    let headers = vec![("host".to_owned(), endpoint.host_header())];
    let response = send(
        &endpoint,
        "GET",
        &endpoint.base_path,
        &headers,
        &[],
        HEADER_LIMIT + 1024,
    )?;
    if response.status == 200 {
        Ok(())
    } else {
        Err(storage_failure(
            "The generated public media URL is not anonymously readable.",
            response.status,
        ))
    }
}

fn accept_delete_status(status: u16) -> Result<(), ApiError> {
    match status {
        200 | 204 | 404 => Ok(()),
        status => Err(storage_failure(
            "Media object could not be deleted.",
            status,
        )),
    }
}

fn storage_failure(message: &str, status: u16) -> ApiError {
    tracing::error!(status, "object storage request failed");
    ApiError::service_unavailable(format!("{message} Object storage returned {status}."))
}

struct Response {
    status: u16,
}

struct Endpoint {
    tls: bool,
    host: String,
    port: u16,
    base_path: String,
}

impl Endpoint {
    fn host_header(&self) -> String {
        let default_port = if self.tls { 443 } else { 80 };
        if self.port == default_port {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

fn parse_endpoint(endpoint: &str) -> Result<Endpoint, ApiError> {
    let (scheme, rest) = endpoint
        .split_once("://")
        .ok_or_else(|| ApiError::internal("Object storage endpoint is invalid."))?;
    let tls = match scheme {
        "http" => false,
        "https" => true,
        _ => return Err(ApiError::internal("Object storage endpoint is invalid.")),
    };
    let (authority, base_path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{}", path.trim_matches('/'))),
        None => (rest, String::new()),
    };
    let authority = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(']') => (
            host.to_owned(),
            port.parse::<u16>()
                .map_err(|_| ApiError::internal("Object storage endpoint is invalid."))?,
        ),
        _ => (authority.to_owned(), if tls { 443 } else { 80 }),
    };
    if host.is_empty() {
        return Err(ApiError::internal("Object storage endpoint is invalid."));
    }
    Ok(Endpoint {
        tls,
        host,
        port,
        base_path: base_path.trim_end_matches('/').to_owned(),
    })
}

fn object_path(settings: &MediaStorageSettings, endpoint: &Endpoint, key: &str) -> String {
    // Uploads persist the complete, prefix-qualified storage key so every
    // backend addresses the same immutable object. Reapplying `key_prefix`
    // here would store S3 objects under `prefix/prefix/...`.
    if settings.path_style {
        let bucket = settings.bucket.trim_matches('/');
        format!("{}/{}/{}", endpoint.base_path, bucket, key)
    } else {
        format!("{}/{}", endpoint.base_path, key)
    }
}

#[allow(clippy::too_many_arguments)]
fn exchange(
    settings: &MediaStorageSettings,
    method: &str,
    key: &str,
    extra_headers: Vec<(String, String)>,
    body: &[u8],
    response_limit: usize,
) -> Result<Response, ApiError> {
    let endpoint = parse_endpoint(&settings.endpoint)?;
    let host_header = if settings.path_style {
        endpoint.host_header()
    } else {
        format!(
            "{}.{}",
            settings.bucket.trim_matches('/'),
            endpoint.host_header()
        )
    };
    let path = object_path(settings, &endpoint, key);
    let headers = authorize(settings, method, &path, &host_header, &extra_headers, body)?;
    send(&endpoint, method, &path, &headers, body, response_limit)
}

fn authorize(
    settings: &MediaStorageSettings,
    method: &str,
    path: &str,
    host_header: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
) -> Result<Vec<(String, String)>, ApiError> {
    authorize_at(
        settings,
        method,
        path,
        host_header,
        extra_headers,
        body,
        Utc::now(),
    )
}

fn authorize_at(
    settings: &MediaStorageSettings,
    method: &str,
    path: &str,
    host_header: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    now: DateTime<Utc>,
) -> Result<Vec<(String, String)>, ApiError> {
    let payload_hash = HEXLOWER.encode(&Sha256::digest(body));
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let scope = format!("{date_stamp}/{}/s3/aws4_request", settings.region.trim());

    let mut headers = vec![
        ("host".to_owned(), host_header.to_owned()),
        ("x-amz-content-sha256".to_owned(), payload_hash.clone()),
        ("x-amz-date".to_owned(), amz_date.clone()),
    ];
    headers.extend(
        extra_headers
            .iter()
            .map(|(name, value)| (name.to_ascii_lowercase(), value.clone())),
    );
    headers.sort_by(|left, right| left.0.cmp(&right.0));

    let canonical_headers = headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect::<String>();
    let signed_headers = headers
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_request = format!(
        "{method}\n{}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        uri_encode(path, false)
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        HEXLOWER.encode(&Sha256::digest(canonical_request.as_bytes()))
    );
    let signing_key = signing_key(
        &settings.secret_access_key,
        &date_stamp,
        &settings.region,
        "s3",
    )?;
    let mut mac = HmacSha256::new_from_slice(&signing_key)
        .map_err(|_| ApiError::internal("Object storage signing key is invalid."))?;
    mac.update(string_to_sign.as_bytes());
    let signature = HEXLOWER.encode(&mac.finalize().into_bytes());

    let mut signed = vec![(
        "authorization".to_owned(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            settings.access_key_id
        ),
    )];
    signed.extend(headers);
    Ok(signed)
}

fn signing_key(
    secret: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
) -> Result<Vec<u8>, ApiError> {
    let secret = format!("AWS4{secret}");
    let mut key = hmac(secret.as_bytes(), date_stamp.as_bytes())?;
    key = hmac(&key, region.trim().as_bytes())?;
    key = hmac(&key, service.as_bytes())?;
    hmac(&key, b"aws4_request")
}

fn hmac(key: &[u8], value: &[u8]) -> Result<Vec<u8>, ApiError> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|_| ApiError::internal("Object storage signing key is invalid."))?;
    mac.update(value);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn uri_encode(value: &str, encode_slash: bool) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            b'/' if !encode_slash => encoded.push('/'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn send(
    endpoint: &Endpoint,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
    response_limit: usize,
) -> Result<Response, ApiError> {
    let address = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(|error| {
            tracing::error!(%error, "object storage address resolution failed");
            ApiError::service_unavailable("Object storage is unreachable.")
        })?
        .next()
        .ok_or_else(|| ApiError::service_unavailable("Object storage is unreachable."))?;
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|error| {
        tracing::error!(%error, "object storage connection failed");
        ApiError::service_unavailable("Object storage is unreachable.")
    })?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok();

    let request = render_request(method, path, headers, body);
    let response = if endpoint.tls {
        let config = tls_config()?;
        let server_name = rustls::pki_types::ServerName::try_from(endpoint.host.clone())
            .map_err(|_| ApiError::internal("Object storage host name is invalid."))?;
        let connection = rustls::ClientConnection::new(config, server_name)
            .map_err(|_| ApiError::service_unavailable("Object storage TLS handshake failed."))?;
        exchange_bytes(
            rustls::StreamOwned::new(connection, stream),
            &request,
            response_limit,
        )?
    } else {
        exchange_bytes(stream, &request, response_limit)?
    };
    parse_response(&response)
}

fn render_request(method: &str, path: &str, headers: &[(String, String)], body: &[u8]) -> Vec<u8> {
    let request_path = if path.is_empty() { "/" } else { path };
    let mut request = Vec::with_capacity(body.len() + 512);
    request.extend_from_slice(
        format!("{method} {} HTTP/1.1\r\n", uri_encode(request_path, false)).as_bytes(),
    );
    for (name, value) in headers {
        request.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    if !headers.iter().any(|(name, _)| name == "content-length") {
        request.extend_from_slice(format!("content-length: {}\r\n", body.len()).as_bytes());
    }
    request.extend_from_slice(b"connection: close\r\n\r\n");
    request.extend_from_slice(body);
    request
}

fn exchange_bytes<S: Read + Write>(
    mut stream: S,
    request: &[u8],
    response_limit: usize,
) -> Result<Vec<u8>, ApiError> {
    stream.write_all(request).map_err(|error| {
        tracing::error!(%error, "object storage request failed");
        ApiError::service_unavailable("Object storage is unreachable.")
    })?;
    stream.flush().ok();
    read_limited(&mut stream, response_limit).map_err(|error| {
        tracing::error!(%error, "object storage response failed");
        ApiError::service_unavailable("Object storage response was invalid.")
    })
}

fn read_limited<R: Read>(reader: &mut R, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut collected = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(collected);
        }
        if collected.len() + read > limit {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "object storage response exceeded its limit",
            ));
        }
        collected.extend_from_slice(&buffer[..read]);
    }
}

fn parse_response(raw: &[u8]) -> Result<Response, ApiError> {
    let separator = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| ApiError::service_unavailable("Object storage response was invalid."))?;
    let head = std::str::from_utf8(&raw[..separator])
        .map_err(|_| ApiError::service_unavailable("Object storage response was invalid."))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| ApiError::service_unavailable("Object storage response was invalid."))?;
    Ok(Response { status })
}

fn tls_config() -> Result<Arc<rustls::ClientConfig>, ApiError> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    if let Some(config) = CONFIG.get() {
        return Ok(config.clone());
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| ApiError::internal("Object storage TLS configuration failed."))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let config = Arc::new(config);
    let _ = CONFIG.set(config.clone());
    Ok(config)
}

#[cfg(test)]
#[path = "s3_tests.rs"]
mod tests;
