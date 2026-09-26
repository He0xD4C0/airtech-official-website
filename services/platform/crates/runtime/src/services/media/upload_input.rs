use std::{fs::OpenOptions, path::PathBuf, time::Duration};

use axum::extract::Multipart;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::{ApiError, MEDIA_DECODE_FAILED};

use super::MAX_MEDIA_UPLOAD_BYTES;

const MEDIA_UPLOAD_BODY_LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedMediaType {
    pub mime: &'static str,
    pub extension: &'static str,
}

const PNG: AcceptedMediaType = AcceptedMediaType {
    mime: "image/png",
    extension: "png",
};
const JPEG: AcceptedMediaType = AcceptedMediaType {
    mime: "image/jpeg",
    extension: "jpg",
};
const WEBP: AcceptedMediaType = AcceptedMediaType {
    mime: "image/webp",
    extension: "webp",
};

pub struct StagedUpload {
    path: PathBuf,
    pub file_name: String,
    pub media_type: AcceptedMediaType,
    pub byte_size: u64,
    pub sha256: String,
}

impl StagedUpload {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for StagedUpload {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, "staged media upload could not be removed");
            }
        }
    }
}

pub async fn stage_upload(multipart: Multipart) -> Result<StagedUpload, ApiError> {
    stage_upload_with_timeout(multipart, MEDIA_UPLOAD_BODY_LIFECYCLE_TIMEOUT).await
}

async fn stage_upload_with_timeout(
    multipart: Multipart,
    lifecycle_timeout: Duration,
) -> Result<StagedUpload, ApiError> {
    tokio::time::timeout(lifecycle_timeout, stage_upload_body(multipart))
        .await
        .map_err(|_| upload_body_timeout())?
}

async fn stage_upload_body(mut multipart: Multipart) -> Result<StagedUpload, ApiError> {
    let mut staged = None;
    while let Some(mut field) = multipart.next_field().await.map_err(multipart_error)? {
        if field.name() != Some("file") {
            return Err(exactly_one_file_part());
        }
        if staged.is_some() {
            return Err(exactly_one_file_part());
        }
        let file_name = sanitize_file_name(field.file_name().unwrap_or("upload"));
        let (path, mut file) = create_temporary_file()?;
        let mut staged_path = StagedPath::new(path);
        let mut size = 0_u64;
        let mut header = Vec::with_capacity(32);
        let mut digest = Sha256::new();
        while let Some(chunk) = field.chunk().await.map_err(multipart_error)? {
            size = size.saturating_add(chunk.len() as u64);
            if size > MAX_MEDIA_UPLOAD_BYTES as u64 {
                return Err(payload_too_large());
            }
            let wanted = 32_usize.saturating_sub(header.len()).min(chunk.len());
            header.extend_from_slice(&chunk[..wanted]);
            digest.update(&chunk);
            file.write_all(&chunk).await.map_err(|error| {
                tracing::error!(%error, "staged media upload write failed");
                ApiError::service_unavailable("Media upload staging is unavailable.")
            })?;
        }
        file.flush().await.map_err(staging_error)?;
        file.sync_data().await.map_err(staging_error)?;
        drop(file);
        let media_type = sniff_media_type(&header).ok_or_else(unsupported_media_type)?;
        staged = Some(StagedUpload {
            path: staged_path.take(),
            file_name,
            media_type,
            byte_size: size,
            sha256: data_encoding::HEXLOWER.encode(&digest.finalize()),
        });
    }
    staged.ok_or_else(exactly_one_file_part)
}

struct StagedPath(Option<PathBuf>);

impl StagedPath {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn take(&mut self) -> PathBuf {
        self.0.take().expect("staged upload path is present")
    }
}

impl Drop for StagedPath {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn exactly_one_file_part() -> ApiError {
    ApiError::bad_request("The upload must contain exactly one `file` part.")
}

fn create_temporary_file() -> Result<(PathBuf, tokio::fs::File), ApiError> {
    let path = std::env::temp_dir().join(format!("airtek-media-{}.partial", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&path).map_err(staging_error)?;
    Ok((path, tokio::fs::File::from_std(file)))
}

fn staging_error(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "media upload staging failed");
    ApiError::service_unavailable("Media upload staging is unavailable.")
}

fn multipart_error(error: axum::extract::multipart::MultipartError) -> ApiError {
    tracing::warn!(%error, "invalid media multipart body");
    ApiError::bad_request("The media multipart body is invalid.")
}

fn payload_too_large() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
        "Payload too large",
        "The uploaded file exceeds the 25 MiB limit.",
    )
}

fn upload_body_timeout() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::REQUEST_TIMEOUT,
        "Request timeout",
        "The media upload body exceeded its bounded lifecycle deadline.",
    )
}

fn unsupported_media_type() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Unsupported media type",
        "Only valid PNG, JPEG, and WebP image bytes are accepted.",
    )
    .with_code(MEDIA_DECODE_FAILED)
}

pub fn sniff_media_type(bytes: &[u8]) -> Option<AcceptedMediaType> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(PNG);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(JPEG);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(WEBP);
    }
    None
}

fn sanitize_file_name(value: &str) -> String {
    let base = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(180)
        .collect::<String>();
    if base.is_empty() {
        "upload".to_owned()
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, time::Duration};

    use axum::{
        body::{Body, Bytes},
        http::{header, Request, StatusCode},
        response::IntoResponse,
        routing::post,
        Router,
    };
    use futures_util::stream;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn sniffer_accepts_only_supported_raster_headers() {
        assert_eq!(sniff_media_type(b"\x89PNG\r\n\x1a\n").unwrap(), PNG);
        assert_eq!(sniff_media_type(b"\xff\xd8\xff\xe0").unwrap(), JPEG);
        assert_eq!(sniff_media_type(b"RIFF\0\0\0\0WEBP").unwrap(), WEBP);
        assert!(sniff_media_type(b"<svg/>").is_none());
    }

    #[tokio::test]
    async fn unsupported_bytes_use_the_stable_decode_problem_type() {
        let response = unsupported_media_type().into_response();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            problem["type"],
            crate::error::problem_type_uri(MEDIA_DECODE_FAILED)
        );
    }

    async fn stage_route(multipart: Multipart) -> Result<StatusCode, ApiError> {
        let _upload = stage_upload(multipart).await?;
        Ok(StatusCode::NO_CONTENT)
    }

    fn test_router() -> Router {
        Router::new().route(
            "/upload",
            post(stage_route).layer(axum::extract::DefaultBodyLimit::max(
                MAX_MEDIA_UPLOAD_BYTES + 64 * 1024,
            )),
        )
    }

    fn timeout_test_router(lifecycle_timeout: Duration) -> Router {
        Router::new().route(
            "/upload",
            post(move |multipart: Multipart| async move {
                let _upload = stage_upload_with_timeout(multipart, lifecycle_timeout).await?;
                Ok::<_, ApiError>(StatusCode::NO_CONTENT)
            })
            .layer(axum::extract::DefaultBodyLimit::max(
                MAX_MEDIA_UPLOAD_BYTES + 64 * 1024,
            )),
        )
    }

    fn multipart_body(boundary: &str, parts: &[(&str, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        for (name, bytes) in parts {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"upload.png\"\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(bytes);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        body
    }

    async fn send_multipart(boundary: &str, body: Vec<u8>) -> axum::response::Response {
        test_router()
            .oneshot(
                Request::post("/upload")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    fn png_sized(byte_size: usize) -> Vec<u8> {
        let mut bytes = vec![0; byte_size];
        bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        bytes
    }

    #[tokio::test]
    async fn multipart_requires_exactly_one_file_part() {
        let png = png_sized(8);
        let unknown = send_multipart(
            "unknown-part",
            multipart_body("unknown-part", &[("caption", &png)]),
        )
        .await;
        assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);

        let duplicate = send_multipart(
            "duplicate-file",
            multipart_body("duplicate-file", &[("file", &png), ("file", &png)]),
        )
        .await;
        assert_eq!(duplicate.status(), StatusCode::BAD_REQUEST);

        for response in [unknown, duplicate] {
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(problem["status"], 400);
            assert_eq!(
                problem["detail"],
                "The upload must contain exactly one `file` part."
            );
        }
    }

    #[tokio::test]
    async fn multipart_file_size_boundary_is_exactly_25_mib() {
        let at_limit = png_sized(MAX_MEDIA_UPLOAD_BYTES);
        let response = send_multipart(
            "at-limit",
            multipart_body("at-limit", &[("file", &at_limit)]),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        drop(at_limit);

        let over_limit = png_sized(MAX_MEDIA_UPLOAD_BYTES + 1);
        let response = send_multipart(
            "over-limit",
            multipart_body("over-limit", &[("file", &over_limit)]),
        )
        .await;
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["status"], 413);
        assert_eq!(
            problem["detail"],
            "The uploaded file exceeds the 25 MiB limit."
        );
    }

    #[tokio::test]
    async fn malformed_multipart_boundary_fails_closed() {
        let response = send_multipart(
            "declared-boundary",
            multipart_body("different-boundary", &[("file", &png_sized(8))]),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["status"], 400);
        assert_eq!(problem["detail"], "The media multipart body is invalid.");
    }

    #[tokio::test(start_paused = true)]
    async fn slow_drip_body_hits_the_application_lifecycle_deadline() {
        let boundary = "slow-drip";
        let mut prefix = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
             filename=\"upload.png\"\r\n\r\n"
        )
        .into_bytes();
        prefix.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        let mut chunks = vec![(Duration::ZERO, prefix)];
        chunks.extend(std::iter::repeat_with(|| (Duration::from_millis(9), vec![0])).take(20));
        let body = Body::from_stream(stream::unfold(
            chunks.into_iter(),
            |mut chunks| async move {
                let (delay, chunk) = chunks.next()?;
                tokio::time::sleep(delay).await;
                Some((Ok::<_, Infallible>(Bytes::from(chunk)), chunks))
            },
        ));
        let response = timeout_test_router(Duration::from_millis(50))
            .oneshot(
                Request::post("/upload")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let problem: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(problem["status"], 408);
        assert_eq!(problem["type"], crate::error::problem_type_uri("408"));
        assert_eq!(
            problem["detail"],
            "The media upload body exceeded its bounded lifecycle deadline."
        );
    }
}
