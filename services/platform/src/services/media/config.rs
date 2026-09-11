use std::{env, fmt, path::PathBuf};

/// Hard ceiling for a single uploaded object. The public delivery path and the
/// multipart reader share it so an oversized body is never buffered twice.
pub const MAX_MEDIA_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

pub const DEFAULT_LOCAL_MEDIA_ROOT: &str = "/var/lib/airtek-media";
pub const DEFAULT_MEDIA_KEY_PREFIX: &str = "media";
pub const DEFAULT_MEDIA_REGION: &str = "us-east-1";

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
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct MediaSettings {
    /// `None` means the deployment has not completely configured object
    /// storage. Admins keep read access to the catalogue and uploads fail
    /// closed.
    pub storage: Option<MediaStorageSettings>,
}

impl MediaSettings {
    pub fn disabled() -> Self {
        Self { storage: None }
    }

    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    fn from_lookup<F>(lookup: F) -> Result<Self, String>
    where
        F: Fn(&str) -> Option<String>,
    {
        let kind = non_empty_from(&lookup, "AIRTEK_MEDIA_STORAGE")
            .unwrap_or_default()
            .to_ascii_lowercase();
        let storage = match kind.as_str() {
            "" => None,
            "local" => Some(MediaStorageSettings {
                kind: MediaStorageKind::Local,
                local_root: PathBuf::from(
                    non_empty_from(&lookup, "AIRTEK_MEDIA_LOCAL_ROOT")
                        .unwrap_or_else(|| DEFAULT_LOCAL_MEDIA_ROOT.to_owned()),
                ),
                endpoint: String::new(),
                region: DEFAULT_MEDIA_REGION.to_owned(),
                bucket: String::new(),
                access_key_id: String::new(),
                secret_access_key: String::new(),
                key_prefix: DEFAULT_MEDIA_KEY_PREFIX.to_owned(),
                path_style: true,
            }),
            "s3" => s3_settings(&lookup)?,
            other => {
                return Err(format!(
                    "AIRTEK_MEDIA_STORAGE must be local or s3, not {other}"
                ))
            }
        };
        if let Some(storage) = &storage {
            if storage.key_prefix.is_empty()
                || storage
                    .key_prefix
                    .split('/')
                    .any(|segment| segment.is_empty() || segment == "." || segment == "..")
            {
                return Err("AIRTEK_MEDIA_S3_KEY_PREFIX must be a relative prefix".into());
            }
            if storage.kind == MediaStorageKind::S3 {
                if !(storage.endpoint.starts_with("http://")
                    || storage.endpoint.starts_with("https://"))
                {
                    return Err(
                        "AIRTEK_MEDIA_S3_ENDPOINT must start with http:// or https://".into(),
                    );
                }
                if storage.endpoint.ends_with('/') {
                    return Err("AIRTEK_MEDIA_S3_ENDPOINT must not end with a slash".into());
                }
            }
        }
        Ok(Self { storage })
    }

    pub fn storage_kind_label(&self) -> Option<&'static str> {
        self.storage.as_ref().map(|storage| storage.kind.label())
    }
}

fn non_empty_from<F>(lookup: &F, name: &str) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn s3_settings<F>(lookup: &F) -> Result<Option<MediaStorageSettings>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let endpoint = non_empty_from(lookup, "AIRTEK_MEDIA_S3_ENDPOINT");
    let bucket = non_empty_from(lookup, "AIRTEK_MEDIA_S3_BUCKET");
    let access_key_id = non_empty_from(lookup, "AIRTEK_MEDIA_S3_ACCESS_KEY_ID");
    let secret_access_key = non_empty_from(lookup, "AIRTEK_MEDIA_S3_SECRET_ACCESS_KEY");
    let key_prefix = non_empty_from(lookup, "AIRTEK_MEDIA_S3_KEY_PREFIX")
        .unwrap_or_else(|| DEFAULT_MEDIA_KEY_PREFIX.to_owned());
    let path_style = bool_value(
        "AIRTEK_MEDIA_S3_PATH_STYLE",
        non_empty_from(lookup, "AIRTEK_MEDIA_S3_PATH_STYLE"),
        true,
    )?;

    if key_prefix.is_empty()
        || key_prefix
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err("AIRTEK_MEDIA_S3_KEY_PREFIX must be a relative prefix".into());
    }
    if let Some(endpoint) = &endpoint {
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            return Err("AIRTEK_MEDIA_S3_ENDPOINT must start with http:// or https://".into());
        }
        if endpoint.ends_with('/') {
            return Err("AIRTEK_MEDIA_S3_ENDPOINT must not end with a slash".into());
        }
    }

    let (endpoint, bucket, access_key_id, secret_access_key) =
        match (endpoint, bucket, access_key_id, secret_access_key) {
            (Some(endpoint), Some(bucket), Some(access_key_id), Some(secret_access_key)) => {
                (endpoint, bucket, access_key_id, secret_access_key)
            }
            _ => {
                tracing::warn!(
                    "incomplete S3 media configuration; object storage remains disabled"
                );
                return Ok(None);
            }
        };
    Ok(Some(MediaStorageSettings {
        kind: MediaStorageKind::S3,
        local_root: PathBuf::from(DEFAULT_LOCAL_MEDIA_ROOT),
        endpoint,
        region: non_empty_from(lookup, "AIRTEK_MEDIA_S3_REGION")
            .unwrap_or_else(|| DEFAULT_MEDIA_REGION.to_owned()),
        bucket,
        access_key_id,
        secret_access_key,
        key_prefix,
        path_style,
    }))
}

fn bool_value(name: &str, value: Option<String>, default: bool) -> Result<bool, String> {
    match value.map(|value| value.to_ascii_lowercase()) {
        None => Ok(default),
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes" | "on") => Ok(true),
        Some(value) if matches!(value.as_str(), "0" | "false" | "no" | "off") => Ok(false),
        Some(value) => Err(format!("{name} must be a boolean, not {value}")),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn from_values(values: &[(&str, &str)]) -> Result<MediaSettings, String> {
        let values = values
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>();
        MediaSettings::from_lookup(|name| values.get(name).cloned())
    }

    #[test]
    fn incomplete_s3_configuration_disables_storage_without_blocking_startup() {
        let required = [
            ("AIRTEK_MEDIA_S3_ENDPOINT", "http://minio:9000"),
            ("AIRTEK_MEDIA_S3_BUCKET", "airtek-media"),
            ("AIRTEK_MEDIA_S3_ACCESS_KEY_ID", "access"),
            ("AIRTEK_MEDIA_S3_SECRET_ACCESS_KEY", "secret"),
        ];
        for missing in 0..required.len() {
            let mut values = vec![("AIRTEK_MEDIA_STORAGE", "s3")];
            values.extend(
                required
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != missing)
                    .map(|(_, value)| *value),
            );
            let settings = from_values(&values).expect("incomplete S3 disables storage");
            assert!(
                settings.storage.is_none(),
                "missing {}",
                required[missing].0
            );
        }
        let settings = from_values(
            &[
                ("AIRTEK_MEDIA_STORAGE", "s3"),
                ("AIRTEK_MEDIA_S3_ENDPOINT", "http://minio:9000"),
                ("AIRTEK_MEDIA_S3_BUCKET", "airtek-media"),
                ("AIRTEK_MEDIA_S3_ACCESS_KEY_ID", "access"),
                ("AIRTEK_MEDIA_S3_SECRET_ACCESS_KEY", "   "),
            ],
        )
        .expect("blank required S3 value disables storage");
        assert!(settings.storage.is_none());
    }

    #[test]
    fn complete_s3_configuration_enables_storage() {
        let settings = from_values(
            &[
                ("AIRTEK_MEDIA_STORAGE", "s3"),
                ("AIRTEK_MEDIA_S3_ENDPOINT", "http://minio:9000"),
                ("AIRTEK_MEDIA_S3_BUCKET", "airtek-media"),
                ("AIRTEK_MEDIA_S3_ACCESS_KEY_ID", "access"),
                ("AIRTEK_MEDIA_S3_SECRET_ACCESS_KEY", "secret"),
                ("AIRTEK_MEDIA_S3_PATH_STYLE", "false"),
            ],
        )
        .expect("complete S3 configuration");
        let storage = settings.storage.expect("S3 is enabled");
        assert_eq!(storage.kind, MediaStorageKind::S3);
        assert!(!storage.path_style);
    }

    #[test]
    fn explicit_invalid_s3_values_still_fail_configuration() {
        for (name, value, expected) in [
            ("AIRTEK_MEDIA_S3_ENDPOINT", "minio:9000", "must start"),
            (
                "AIRTEK_MEDIA_S3_KEY_PREFIX",
                "media/../private",
                "relative prefix",
            ),
            (
                "AIRTEK_MEDIA_S3_PATH_STYLE",
                "sometimes",
                "must be a boolean",
            ),
        ] {
            let error = from_values(&[("AIRTEK_MEDIA_STORAGE", "s3"), (name, value)])
                .expect_err("explicit invalid S3 values must fail");
            assert!(error.contains(expected), "{name}: {error}");
        }
    }
}
