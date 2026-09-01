use std::{env, fmt, net::IpAddr, str::FromStr};

use base64::{engine::general_purpose, Engine as _};
use thiserror::Error;

pub const API_PORT: u16 = 8080;

#[derive(Clone)]
pub struct Config {
    pub host: IpAddr,
    pub public_origin: String,
    pub admin_origin: String,
    pub database_url: Option<String>,
    /// One-time deployment secret. It is accepted only by `/auth/setup` while
    /// the user table is empty; it never authenticates normal API requests.
    pub admin_bootstrap_token: Option<String>,
    /// Deployment-provided AEAD key used only to seal TOTP secrets at rest.
    /// The wrapper intentionally redacts its Debug representation.
    pub totp_encryption_key: Option<TotpEncryptionKey>,
    /// Dedicated deployment key used only for short-lived content preview
    /// signatures. It is deliberately independent from the TOTP AEAD key.
    pub preview_signing_key: Option<PreviewSigningKey>,
    /// Reverse-proxy networks that are allowed to supply an X-Forwarded-For
    /// chain. An unlisted direct peer can never influence the client source.
    pub trusted_proxy_cidrs: Vec<IpCidr>,
    pub production: bool,
}

impl fmt::Debug for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Config")
            .field("host", &self.host)
            .field("public_origin", &self.public_origin)
            .field("admin_origin", &self.admin_origin)
            .field(
                "database_url",
                &self.database_url.as_ref().map(|_| "[configured]"),
            )
            .field(
                "admin_bootstrap_token",
                &self.admin_bootstrap_token.as_ref().map(|_| "[configured]"),
            )
            .field("totp_encryption_key", &self.totp_encryption_key)
            .field("preview_signing_key", &self.preview_signing_key)
            .field("trusted_proxy_cidrs", &self.trusted_proxy_cidrs)
            .field("production", &self.production)
            .finish()
    }
}

#[derive(Clone)]
pub struct TotpEncryptionKey([u8; 32]);

impl TotpEncryptionKey {
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for TotpEncryptionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

#[derive(Clone)]
pub struct PreviewSigningKey([u8; 32]);

impl PreviewSigningKey {
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for PreviewSigningKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpCidr {
    network: IpAddr,
    prefix_len: u8,
}

impl IpCidr {
    pub fn contains(self, address: IpAddr) -> bool {
        match (self.network, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                let prefix = u32::from(self.prefix_len);
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                u32::from(network) & mask == u32::from(address) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                let prefix = u32::from(self.prefix_len);
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                u128::from(network) & mask == u128::from(address) & mask
            }
            _ => false,
        }
    }
}

impl FromStr for IpCidr {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (address, prefix_len) = match value.split_once('/') {
            Some((address, prefix)) => {
                let address = address.parse::<IpAddr>().map_err(|_| ())?;
                let prefix_len = prefix.parse::<u8>().map_err(|_| ())?;
                (address, prefix_len)
            }
            None => {
                let address = value.parse::<IpAddr>().map_err(|_| ())?;
                let prefix_len = if address.is_ipv4() { 32 } else { 128 };
                (address, prefix_len)
            }
        };
        let valid = match address {
            IpAddr::V4(_) => prefix_len <= 32,
            IpAddr::V6(_) => prefix_len <= 128,
        };
        valid
            .then_some(Self {
                network: address,
                prefix_len,
            })
            .ok_or(())
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("AIRTEK_API_HOST is not a valid IP address: {0}")]
    InvalidHost(String),
    #[error("{0} must be an absolute http(s) origin without a path")]
    InvalidOrigin(&'static str),
    #[error("AIRTEK_ADMIN_BOOTSTRAP_TOKEN must contain at least 24 characters when configured")]
    WeakBootstrapToken,
    #[error("AIRTEK_TOTP_ENCRYPTION_KEY must be Base64 for exactly 32 bytes")]
    InvalidTotpEncryptionKey,
    #[error("AIRTEK_TOTP_ENCRYPTION_KEY is required in production builds")]
    MissingTotpEncryptionKey,
    #[error("AIRTEK_PREVIEW_SIGNING_KEY must be Base64 for exactly 32 bytes")]
    InvalidPreviewSigningKey,
    #[error("AIRTEK_PREVIEW_SIGNING_KEY is required in production builds")]
    MissingPreviewSigningKey,
    #[error("AIRTEK_TRUSTED_PROXY_CIDRS contains an invalid IP/CIDR: {0}")]
    InvalidTrustedProxyCidr(String),
    #[error("a devtools build requires DATABASE_URL or AIRTEK_ADMIN_BOOTSTRAP_TOKEN so an authenticated administrator can be established")]
    MissingDevtoolsAuthentication,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let host_raw = env::var("AIRTEK_API_HOST").unwrap_or_else(|_| "0.0.0.0".into());
        let host =
            IpAddr::from_str(&host_raw).map_err(|_| ConfigError::InvalidHost(host_raw.clone()))?;
        let public_origin =
            env::var("AIRTEK_PUBLIC_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".into());
        let admin_origin =
            env::var("AIRTEK_ADMIN_ORIGIN").unwrap_or_else(|_| "http://localhost:3100".into());
        validate_origin("AIRTEK_PUBLIC_ORIGIN", &public_origin)?;
        validate_origin("AIRTEK_ADMIN_ORIGIN", &admin_origin)?;

        let production = cfg!(feature = "production");
        let database_url = non_empty_env("DATABASE_URL");
        let admin_bootstrap_token = non_empty_env("AIRTEK_ADMIN_BOOTSTRAP_TOKEN");
        let totp_encryption_key = non_empty_env("AIRTEK_TOTP_ENCRYPTION_KEY")
            .map(|value| parse_totp_encryption_key(&value))
            .transpose()?;
        let preview_signing_key = non_empty_env("AIRTEK_PREVIEW_SIGNING_KEY")
            .map(|value| parse_preview_signing_key(&value))
            .transpose()?;
        let trusted_proxy_cidrs = parse_trusted_proxy_cidrs(
            env::var("AIRTEK_TRUSTED_PROXY_CIDRS")
                .ok()
                .as_deref()
                .unwrap_or_default(),
        )?;
        if admin_bootstrap_token
            .as_ref()
            .is_some_and(|token| token.len() < 24)
        {
            return Err(ConfigError::WeakBootstrapToken);
        }
        if cfg!(feature = "devtools") && database_url.is_none() && admin_bootstrap_token.is_none() {
            return Err(ConfigError::MissingDevtoolsAuthentication);
        }
        require_production_keys(
            production,
            totp_encryption_key.as_ref(),
            preview_signing_key.as_ref(),
        )?;

        Ok(Self {
            host,
            public_origin,
            admin_origin,
            database_url,
            admin_bootstrap_token,
            totp_encryption_key,
            preview_signing_key,
            trusted_proxy_cidrs,
            production,
        })
    }

    pub fn for_test() -> Self {
        Self {
            host: IpAddr::from([127, 0, 0, 1]),
            public_origin: "http://localhost:3000".into(),
            admin_origin: "http://localhost:3100".into(),
            database_url: None,
            admin_bootstrap_token: Some("test-bootstrap-token-please-change".into()),
            totp_encryption_key: Some(TotpEncryptionKey([0x42; 32])),
            preview_signing_key: Some(PreviewSigningKey([0x24; 32])),
            trusted_proxy_cidrs: Vec::new(),
            production: false,
        }
    }

    pub fn bind_address(&self) -> (IpAddr, u16) {
        (self.host, API_PORT)
    }
}

fn parse_totp_encryption_key(value: &str) -> Result<TotpEncryptionKey, ConfigError> {
    decode_32_byte_key(value)
        .map(TotpEncryptionKey)
        .map_err(|_| ConfigError::InvalidTotpEncryptionKey)
}

fn parse_preview_signing_key(value: &str) -> Result<PreviewSigningKey, ConfigError> {
    decode_32_byte_key(value)
        .map(PreviewSigningKey)
        .map_err(|_| ConfigError::InvalidPreviewSigningKey)
}

fn decode_32_byte_key(value: &str) -> Result<[u8; 32], ()> {
    general_purpose::STANDARD
        .decode(value)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(value))
        .map_err(|_| ())?
        .try_into()
        .map_err(|_| ())
}

fn require_production_keys(
    production: bool,
    totp_encryption_key: Option<&TotpEncryptionKey>,
    preview_signing_key: Option<&PreviewSigningKey>,
) -> Result<(), ConfigError> {
    if production && totp_encryption_key.is_none() {
        return Err(ConfigError::MissingTotpEncryptionKey);
    }
    if production && preview_signing_key.is_none() {
        return Err(ConfigError::MissingPreviewSigningKey);
    }
    Ok(())
}

fn parse_trusted_proxy_cidrs(value: &str) -> Result<Vec<IpCidr>, ConfigError> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse()
                .map_err(|_| ConfigError::InvalidTrustedProxyCidr(value.to_owned()))
        })
        .collect()
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn validate_origin(name: &'static str, value: &str) -> Result<(), ConfigError> {
    let valid_scheme = value.starts_with("http://") || value.starts_with("https://");
    let remainder = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or_default();
    if !valid_scheme || remainder.is_empty() || remainder.contains('/') || value.ends_with('/') {
        return Err(ConfigError::InvalidOrigin(name));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_proxy_cidrs_support_ipv4_ipv6_and_exact_addresses() {
        let cidrs = parse_trusted_proxy_cidrs("172.28.0.0/24, 2001:db8::/32, 192.0.2.10")
            .expect("trusted proxy CIDRs");
        assert!(cidrs[0].contains("172.28.0.10".parse().unwrap()));
        assert!(!cidrs[0].contains("172.28.1.10".parse().unwrap()));
        assert!(cidrs[1].contains("2001:db8::42".parse().unwrap()));
        assert!(!cidrs[1].contains("2001:db9::42".parse().unwrap()));
        assert!(cidrs[2].contains("192.0.2.10".parse().unwrap()));
        assert!(!cidrs[2].contains("192.0.2.11".parse().unwrap()));
    }

    #[test]
    fn invalid_trusted_proxy_cidr_is_rejected() {
        assert!(matches!(
            parse_trusted_proxy_cidrs("172.28.0.0/64"),
            Err(ConfigError::InvalidTrustedProxyCidr(_))
        ));
    }

    #[test]
    fn totp_key_requires_exactly_32_decoded_bytes_and_is_redacted() {
        let encoded = general_purpose::STANDARD.encode([7_u8; 32]);
        let key = parse_totp_encryption_key(&encoded).expect("valid key");
        assert_eq!(key.as_bytes(), &[7_u8; 32]);
        assert_eq!(format!("{key:?}"), "[redacted]");
        assert!(parse_totp_encryption_key("too-short").is_err());
    }

    #[test]
    fn preview_key_is_independent_redacted_and_required_in_production() {
        let encoded = general_purpose::STANDARD.encode([9_u8; 32]);
        let key = parse_preview_signing_key(&encoded).expect("valid key");
        assert_eq!(key.as_bytes(), &[9_u8; 32]);
        assert_eq!(format!("{key:?}"), "[redacted]");
        assert!(parse_preview_signing_key("too-short").is_err());
        let totp_key = TotpEncryptionKey([7_u8; 32]);
        assert!(matches!(
            require_production_keys(true, Some(&totp_key), None),
            Err(ConfigError::MissingPreviewSigningKey)
        ));
        assert!(require_production_keys(false, Some(&totp_key), None).is_ok());
    }
}
