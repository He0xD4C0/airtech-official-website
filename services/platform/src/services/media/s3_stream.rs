use std::{
    io::{self, Cursor, ErrorKind, Read, Write},
    net::{TcpStream, ToSocketAddrs},
};

use axum::body::{Body, Bytes};
use futures_util::stream;
use tokio::sync::{mpsc, oneshot};

use crate::error::ApiError;

use super::super::storage::MediaObject;
use super::{
    authorize, object_path, parse_endpoint, render_request, storage_failure, tls_config, Endpoint,
    CONNECT_TIMEOUT, HEADER_LIMIT, IO_TIMEOUT,
};

const BODY_CHUNK_BYTES: usize = 64 * 1024;
const BODY_CHANNEL_CAPACITY: usize = 2;
const CHUNK_LINE_LIMIT: usize = 8 * 1024;

type BodyItem = Result<Bytes, io::Error>;

pub(super) async fn get(
    settings: &super::super::config::MediaStorageSettings,
    key: &str,
    expected_size: u64,
) -> Result<MediaObject, ApiError> {
    let settings = settings.clone();
    let key = key.to_owned();
    let (head_sender, head_receiver) = oneshot::channel();
    let (body_sender, body_receiver) = mpsc::channel(BODY_CHANNEL_CAPACITY);
    let panic_sender = body_sender.clone();
    let worker = tokio::task::spawn_blocking(move || {
        stream_get_blocking(&settings, &key, expected_size, head_sender, body_sender);
    });
    tokio::spawn(async move {
        if let Err(error) = worker.await {
            tracing::error!(%error, "object storage streaming worker failed");
            let _ = panic_sender
                .send(Err(io::Error::other(
                    "object storage streaming worker failed",
                )))
                .await;
        }
    });
    head_receiver
        .await
        .map_err(|_| ApiError::service_unavailable("Object storage response was interrupted."))??;

    let body = Body::from_stream(stream::unfold(body_receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    }));
    Ok(MediaObject {
        body,
        content_length: expected_size,
    })
}

fn stream_get_blocking(
    settings: &super::super::config::MediaStorageSettings,
    key: &str,
    expected_size: u64,
    head_sender: oneshot::Sender<Result<(), ApiError>>,
    body_sender: mpsc::Sender<BodyItem>,
) {
    let setup = (|| {
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
        let headers = authorize(settings, "GET", &path, &host_header, &[], &[])?;
        let request = render_request("GET", &path, &headers, &[]);
        let socket = connect(&endpoint)?;
        Ok::<_, ApiError>((endpoint, request, socket))
    })();

    let (endpoint, request, socket) = match setup {
        Ok(setup) => setup,
        Err(error) => {
            let _ = head_sender.send(Err(error));
            return;
        }
    };
    if endpoint.tls {
        let tls = (|| {
            let server_name = rustls::pki_types::ServerName::try_from(endpoint.host.clone())
                .map_err(|_| ApiError::internal("Object storage host name is invalid."))?;
            let connection =
                rustls::ClientConnection::new(tls_config()?, server_name).map_err(|_| {
                    ApiError::service_unavailable("Object storage TLS handshake failed.")
                })?;
            Ok::<_, ApiError>(rustls::StreamOwned::new(connection, socket))
        })();
        match tls {
            Ok(stream) => {
                stream_connection(stream, &request, expected_size, head_sender, body_sender)
            }
            Err(error) => {
                let _ = head_sender.send(Err(error));
            }
        }
    } else {
        stream_connection(socket, &request, expected_size, head_sender, body_sender);
    }
}

fn connect(endpoint: &Endpoint) -> Result<TcpStream, ApiError> {
    let address = (endpoint.host.as_str(), endpoint.port)
        .to_socket_addrs()
        .map_err(|error| {
            tracing::error!(%error, "object storage address resolution failed");
            ApiError::service_unavailable("Object storage is unreachable.")
        })?
        .next()
        .ok_or_else(|| ApiError::service_unavailable("Object storage is unreachable."))?;
    let socket = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|error| {
        tracing::error!(%error, "object storage connection failed");
        ApiError::service_unavailable("Object storage is unreachable.")
    })?;
    socket.set_read_timeout(Some(IO_TIMEOUT)).ok();
    socket.set_write_timeout(Some(IO_TIMEOUT)).ok();
    Ok(socket)
}

fn stream_connection<S: Read + Write>(
    mut stream: S,
    request: &[u8],
    expected_size: u64,
    head_sender: oneshot::Sender<Result<(), ApiError>>,
    body_sender: mpsc::Sender<BodyItem>,
) {
    let prepared = (|| {
        stream.write_all(request).map_err(storage_io_failure)?;
        stream.flush().map_err(storage_io_failure)?;
        let (head, prefix) = read_response_head(&mut stream)?;
        match head.status {
            200 => {}
            404 => return Err(ApiError::not_found("Media object was not found.")),
            status => return Err(storage_failure("Media object could not be read.", status)),
        }
        let framing = head.framing(expected_size)?;
        Ok::<_, ApiError>((framing, prefix))
    })();
    let (framing, prefix) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = head_sender.send(Err(error));
            return;
        }
    };
    if head_sender.send(Ok(())).is_err() {
        return;
    }

    let mut reader = PrefixedReader::new(prefix, stream);
    if let Err(error) = stream_body(&mut reader, framing, expected_size, &body_sender) {
        tracing::error!(%error, "object storage response body failed");
        let _ = body_sender.blocking_send(Err(error));
    }
}

fn storage_io_failure(error: io::Error) -> ApiError {
    tracing::error!(%error, "object storage request failed");
    ApiError::service_unavailable("Object storage is unreachable.")
}

struct ResponseHead {
    status: u16,
    content_length: Option<u64>,
    transfer_encoding: Option<String>,
}

impl ResponseHead {
    fn framing(&self, expected_size: u64) -> Result<Framing, ApiError> {
        if let Some(encoding) = &self.transfer_encoding {
            if self.content_length.is_some()
                || !encoding
                    .split(',')
                    .all(|value| value.trim().eq_ignore_ascii_case("chunked"))
            {
                return Err(invalid_response());
            }
            return Ok(Framing::Chunked);
        }
        match self.content_length {
            Some(length) if length == expected_size => Ok(Framing::ContentLength),
            Some(length) => {
                tracing::error!(length, expected_size, "object storage length mismatch");
                Err(ApiError::service_unavailable(
                    "Stored media object does not match its catalogue metadata.",
                ))
            }
            None => Ok(Framing::CloseDelimited),
        }
    }
}

#[derive(Clone, Copy)]
enum Framing {
    ContentLength,
    Chunked,
    CloseDelimited,
}

fn read_response_head<R: Read>(reader: &mut R) -> Result<(ResponseHead, Vec<u8>), ApiError> {
    let mut received = Vec::with_capacity(8 * 1024);
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        if let Some(separator) = received.windows(4).position(|part| part == b"\r\n\r\n") {
            let prefix = received.split_off(separator + 4);
            received.truncate(separator);
            return parse_response_head(&received).map(|head| (head, prefix));
        }
        if received.len() == HEADER_LIMIT {
            return Err(invalid_response());
        }
        let allowance = (HEADER_LIMIT - received.len()).min(buffer.len());
        let read = reader
            .read(&mut buffer[..allowance])
            .map_err(|_| invalid_response())?;
        if read == 0 {
            return Err(invalid_response());
        }
        received.extend_from_slice(&buffer[..read]);
    }
}

fn parse_response_head(raw: &[u8]) -> Result<ResponseHead, ApiError> {
    let head = std::str::from_utf8(raw).map_err(|_| invalid_response())?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(invalid_response)?;
    let mut content_length = None;
    let mut transfer_encoding = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(invalid_response)?;
        if name.eq_ignore_ascii_case("content-length") {
            let length = value
                .trim()
                .parse::<u64>()
                .map_err(|_| invalid_response())?;
            if content_length
                .replace(length)
                .is_some_and(|prior| prior != length)
            {
                return Err(invalid_response());
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding")
            && transfer_encoding.replace(value.trim().to_owned()).is_some()
        {
            return Err(invalid_response());
        }
    }
    Ok(ResponseHead {
        status,
        content_length,
        transfer_encoding,
    })
}

fn invalid_response() -> ApiError {
    ApiError::service_unavailable("Object storage response was invalid.")
}

struct PrefixedReader<R> {
    prefix: Cursor<Vec<u8>>,
    inner: R,
}

impl<R> PrefixedReader<R> {
    fn new(prefix: Vec<u8>, inner: R) -> Self {
        Self {
            prefix: Cursor::new(prefix),
            inner,
        }
    }
}

impl<R: Read> Read for PrefixedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let prefixed = self.prefix.read(buffer)?;
        if prefixed > 0 {
            Ok(prefixed)
        } else {
            self.inner.read(buffer)
        }
    }
}

fn stream_body<R: Read>(
    reader: &mut R,
    framing: Framing,
    expected_size: u64,
    sender: &mpsc::Sender<BodyItem>,
) -> io::Result<()> {
    match framing {
        Framing::ContentLength => forward_exact(reader, expected_size, sender).map(|_| ()),
        Framing::Chunked => forward_chunked(reader, expected_size, sender),
        Framing::CloseDelimited => forward_to_eof(reader, expected_size, sender),
    }
}

fn forward_exact<R: Read>(
    reader: &mut R,
    mut remaining: u64,
    sender: &mpsc::Sender<BodyItem>,
) -> io::Result<bool> {
    let mut buffer = vec![0_u8; BODY_CHUNK_BYTES];
    while remaining > 0 {
        let wanted = remaining.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..wanted])?;
        if read == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "object storage response ended early",
            ));
        }
        remaining -= read as u64;
        if sender
            .blocking_send(Ok(Bytes::copy_from_slice(&buffer[..read])))
            .is_err()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn forward_to_eof<R: Read>(
    reader: &mut R,
    expected_size: u64,
    sender: &mpsc::Sender<BodyItem>,
) -> io::Result<()> {
    let mut delivered = 0_u64;
    let mut buffer = vec![0_u8; BODY_CHUNK_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        delivered = delivered.saturating_add(read as u64);
        if delivered > expected_size {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "object storage response exceeded catalogue size",
            ));
        }
        if sender
            .blocking_send(Ok(Bytes::copy_from_slice(&buffer[..read])))
            .is_err()
        {
            return Ok(());
        }
    }
    if delivered != expected_size {
        return Err(io::Error::new(
            ErrorKind::UnexpectedEof,
            "object storage response ended early",
        ));
    }
    Ok(())
}

fn forward_chunked<R: Read>(
    reader: &mut R,
    expected_size: u64,
    sender: &mpsc::Sender<BodyItem>,
) -> io::Result<()> {
    let mut delivered = 0_u64;
    loop {
        let line = read_crlf_line(reader, CHUNK_LINE_LIMIT)?;
        let line = std::str::from_utf8(&line)
            .map_err(|_| io::Error::new(ErrorKind::InvalidData, "invalid chunk size"))?;
        let size = u64::from_str_radix(line.split(';').next().unwrap_or_default().trim(), 16)
            .map_err(|_| io::Error::new(ErrorKind::InvalidData, "invalid chunk size"))?;
        if size == 0 {
            read_trailers(reader)?;
            if delivered != expected_size {
                return Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "object storage response length did not match catalogue size",
                ));
            }
            return Ok(());
        }
        delivered = delivered.saturating_add(size);
        if delivered > expected_size {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "object storage response exceeded catalogue size",
            ));
        }
        if !forward_exact(reader, size, sender)? {
            return Ok(());
        }
        if !read_crlf_line(reader, 2)?.is_empty() {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "chunk data was not terminated correctly",
            ));
        }
    }
}

fn read_trailers<R: Read>(reader: &mut R) -> io::Result<()> {
    let mut total = 0_usize;
    loop {
        let line = read_crlf_line(reader, CHUNK_LINE_LIMIT)?;
        if line.is_empty() {
            return Ok(());
        }
        total = total.saturating_add(line.len() + 2);
        if total > HEADER_LIMIT {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "object storage trailers were too large",
            ));
        }
    }
}

fn read_crlf_line<R: Read>(reader: &mut R, limit: usize) -> io::Result<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        if reader.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "object storage chunk framing ended early",
            ));
        }
        line.push(byte[0]);
        if line.ends_with(b"\r\n") {
            line.truncate(line.len() - 2);
            return Ok(line);
        }
        if line.len() > limit {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "object storage chunk line was too large",
            ));
        }
    }
}
