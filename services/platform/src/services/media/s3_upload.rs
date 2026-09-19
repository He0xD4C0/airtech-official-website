use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
};

use crate::error::ApiError;
use crate::services::media::config::MediaStorageSettings;

use super::*;

pub(super) fn put(
    settings: &MediaStorageSettings,
    key: &str,
    content_type: &str,
    path: &Path,
    byte_size: u64,
    sha256: &str,
) -> Result<(), ApiError> {
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
    let object_path = object_path(settings, &endpoint, key);
    let extra_headers = vec![
        ("content-length".to_owned(), byte_size.to_string()),
        ("content-type".to_owned(), content_type.to_owned()),
    ];
    let headers = authorize_hash(
        settings,
        "PUT",
        &object_path,
        &host_header,
        &extra_headers,
        sha256,
    )?;
    let mut file = File::open(path).map_err(file_error)?;
    if file.metadata().map_err(file_error)?.len() != byte_size {
        return Err(ApiError::service_unavailable(
            "Staged media changed before object storage upload.",
        ));
    }
    let response = send_file(
        &endpoint,
        &object_path,
        &headers,
        &mut file,
        byte_size,
        HEADER_LIMIT,
    )?;
    match response.status {
        200 => Ok(()),
        status => Err(storage_failure("Media object could not be stored.", status)),
    }
}

fn send_file(
    endpoint: &Endpoint,
    path: &str,
    headers: &[(String, String)],
    file: &mut File,
    byte_size: u64,
    response_limit: usize,
) -> Result<Response, ApiError> {
    let address = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(connection_error)?
        .next()
        .ok_or_else(|| ApiError::service_unavailable("Object storage is unreachable."))?;
    let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(connection_error)?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok();
    let request_head = render_request("PUT", path, headers, &[]);
    let response = if endpoint.tls {
        let config = tls_config()?;
        let server_name = rustls::pki_types::ServerName::try_from(endpoint.host.clone())
            .map_err(|_| ApiError::internal("Object storage host name is invalid."))?;
        let connection = rustls::ClientConnection::new(config, server_name)
            .map_err(|_| ApiError::service_unavailable("Object storage TLS handshake failed."))?;
        exchange_file(
            rustls::StreamOwned::new(connection, stream),
            &request_head,
            file,
            byte_size,
            response_limit,
        )?
    } else {
        exchange_file(stream, &request_head, file, byte_size, response_limit)?
    };
    parse_response(&response)
}

fn exchange_file<S: Read + Write>(
    mut stream: S,
    request_head: &[u8],
    file: &mut File,
    byte_size: u64,
    response_limit: usize,
) -> Result<Vec<u8>, ApiError> {
    stream.write_all(request_head).map_err(connection_error)?;
    let copied = std::io::copy(&mut file.take(byte_size), &mut stream).map_err(connection_error)?;
    if copied != byte_size {
        return Err(ApiError::service_unavailable(
            "Staged media ended before its expected length.",
        ));
    }
    stream.flush().map_err(connection_error)?;
    read_limited(&mut stream, response_limit).map_err(|error| {
        tracing::error!(%error, "object storage response failed");
        ApiError::service_unavailable("Object storage response was invalid.")
    })
}

fn file_error(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "staged media could not be opened");
    ApiError::service_unavailable("Media upload staging is unavailable.")
}

fn connection_error(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "object storage request failed");
    ApiError::service_unavailable("Object storage is unreachable.")
}
