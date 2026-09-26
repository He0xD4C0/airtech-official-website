use std::{fs::OpenOptions, path::PathBuf};

use futures_util::StreamExt;
use reqwest::header;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::media::MAX_ATTACHMENT_BYTES;

const PROBE_PREFIX_BYTES: usize = 512;
const OFFICE_MARKER_TAIL: usize = 4;

pub struct DownloadedAsset {
    path: PathBuf,
    pub byte_size: u64,
    pub sha256: String,
    pub content_type: Option<String>,
    probe: AssetProbe,
}

impl DownloadedAsset {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn probe(&self) -> &AssetProbe {
        &self.probe
    }
}

impl Drop for DownloadedAsset {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, "staged Feishu attachment could not be removed");
            }
        }
    }
}

pub struct AssetProbe {
    prefix: Vec<u8>,
    has_word_path: bool,
    has_excel_path: bool,
}

impl AssetProbe {
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }

    pub fn has_office_path(&self, path: &[u8]) -> bool {
        match path {
            b"word/" => self.has_word_path,
            b"xl/" => self.has_excel_path,
            _ => false,
        }
    }

    #[cfg(test)]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut builder = ProbeBuilder::default();
        builder.observe(bytes);
        builder.finish()
    }
}

#[derive(Default)]
struct ProbeBuilder {
    prefix: Vec<u8>,
    tail: Vec<u8>,
    has_word_path: bool,
    has_excel_path: bool,
}

impl ProbeBuilder {
    fn observe(&mut self, chunk: &[u8]) {
        let wanted = PROBE_PREFIX_BYTES
            .saturating_sub(self.prefix.len())
            .min(chunk.len());
        self.prefix.extend_from_slice(&chunk[..wanted]);
        let mut scan = Vec::with_capacity(self.tail.len() + chunk.len());
        scan.extend_from_slice(&self.tail);
        scan.extend_from_slice(chunk);
        self.has_word_path |= contains(&scan, b"word/");
        self.has_excel_path |= contains(&scan, b"xl/");
        let tail_start = scan.len().saturating_sub(OFFICE_MARKER_TAIL);
        self.tail.clear();
        self.tail.extend_from_slice(&scan[tail_start..]);
    }

    fn finish(self) -> AssetProbe {
        AssetProbe {
            prefix: self.prefix,
            has_word_path: self.has_word_path,
            has_excel_path: self.has_excel_path,
        }
    }
}

pub(super) async fn stage_asset(response: reqwest::Response) -> Result<DownloadedAsset, ApiError> {
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    validate_attachment_length(response.content_length())?;
    let (path, mut file) = create_temporary_file()?;
    let mut staged_path = StagedPath::new(path);
    let mut stream = response.bytes_stream();
    let mut byte_size = 0_u64;
    let mut digest = Sha256::new();
    let mut probe = ProbeBuilder::default();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(super::network_error)?;
        byte_size = byte_size.saturating_add(chunk.len() as u64);
        if byte_size > MAX_ATTACHMENT_BYTES as u64 {
            return Err(attachment_too_large());
        }
        digest.update(&chunk);
        probe.observe(&chunk);
        file.write_all(&chunk).await.map_err(staging_error)?;
    }
    if byte_size == 0 {
        return Err(ApiError::service_unavailable(
            "Feishu returned an empty attachment.",
        ));
    }
    file.flush().await.map_err(staging_error)?;
    file.sync_data().await.map_err(staging_error)?;
    drop(file);
    Ok(DownloadedAsset {
        path: staged_path.take(),
        byte_size,
        sha256: format!("{:x}", digest.finalize()),
        content_type,
        probe: probe.finish(),
    })
}

fn create_temporary_file() -> Result<(PathBuf, tokio::fs::File), ApiError> {
    let path = std::env::temp_dir().join(format!("airtek-feishu-{}.partial", Uuid::new_v4()));
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
    tracing::error!(%error, "Feishu attachment staging failed");
    ApiError::service_unavailable("Feishu attachment staging is unavailable.")
}

fn attachment_too_large() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
        "Payload too large",
        "The Feishu attachment exceeds the 100 MiB limit.",
    )
}

pub(super) fn validate_attachment_length(content_length: Option<u64>) -> Result<(), ApiError> {
    if content_length.is_some_and(|length| length > MAX_ATTACHMENT_BYTES as u64) {
        Err(attachment_too_large())
    } else {
        Ok(())
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

struct StagedPath(Option<PathBuf>);

impl StagedPath {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn take(&mut self) -> PathBuf {
        self.0.take().expect("staged attachment path is present")
    }
}

impl Drop for StagedPath {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProbeBuilder;

    #[test]
    fn office_markers_are_detected_across_network_chunk_boundaries() {
        let mut builder = ProbeBuilder::default();
        builder.observe(b"PK\x03\x04payload-wo");
        builder.observe(b"rd/document.xml-xl");
        builder.observe(b"/workbook.xml");
        let probe = builder.finish();

        assert!(probe.has_office_path(b"word/"));
        assert!(probe.has_office_path(b"xl/"));
    }
}
