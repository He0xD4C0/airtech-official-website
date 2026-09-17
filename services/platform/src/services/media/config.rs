use std::{fmt, path::PathBuf};

/// Hard ceiling for a single uploaded object. The public delivery path and the
/// multipart reader share it so an oversized body is never buffered twice.
pub const MAX_MEDIA_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

pub const DEFAULT_LOCAL_MEDIA_ROOT: &str = "/var/lib/airtek-media";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaStorageKind {
    Local,
    S3,
}

impl MediaStorageKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
        }
    }
}

#[derive(Clone)]
pub struct MediaStorageSettings {
    pub kind: MediaStorageKind,
    pub local_root: PathBuf,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub key_prefix: String,
    pub path_style: bool,
    pub public_base_url: String,
}

impl fmt::Debug for MediaStorageSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MediaStorageSettings")
            .field("kind", &self.kind)
            .field("local_root", &self.local_root)
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .field("access_key_id", &"[configured]")
            .field("secret_access_key", &"[configured]")
            .field("key_prefix", &self.key_prefix)
            .field("path_style", &self.path_style)
            .field("public_base_url", &self.public_base_url)
            .finish()
    }
}

/// Internal test seam only. Runtime S3 settings are loaded from PostgreSQL by
/// the object-storage service, never from process environment variables.
#[derive(Clone, Debug)]
pub struct MediaSettings {
    pub storage: Option<MediaStorageSettings>,
}

impl MediaSettings {
    pub fn disabled() -> Self {
        Self { storage: None }
    }

    pub fn storage_kind_label(&self) -> Option<&'static str> {
        self.storage.as_ref().map(|storage| storage.kind.label())
    }
}
