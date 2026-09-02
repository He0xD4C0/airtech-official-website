use std::{collections::BTreeSet, env, fmt, net::IpAddr, str::FromStr};

use base64::{engine::general_purpose, Engine as _};
use thiserror::Error;

pub const API_PORT: u16 = 8080;

/// Exact, deployment-owned authority registration for a production Product
/// Master import. A checksum alone is insufficient: the mapping and accepted
/// row/error cardinality are part of the reviewed source decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovedProductMaster {
    pub sha256: String,
    pub mapping_version: String,
    pub expected_valid_rows: i64,
    pub expected_error_rows: i64,
}

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
    /// Independent AEAD key used only for the short-lived encrypted replay of
    /// one-time invitation tokens. The database never stores a plaintext
    /// invitation token, while a client retry can still recover the exact
    /// successful response.
    pub invitation_replay_encryption_key: Option<InvitationReplayEncryptionKey>,
    /// Independent AES-256-GCM key for confidential Product Master staging
    /// fields such as price. It is never reused for authentication secrets.
    pub product_staging_encryption_key: Option<ProductStagingEncryptionKey>,
    /// Independent HMAC key for analytics visit capabilities. Keeping it
    /// separate prevents an analytics token from becoming a signing oracle for
    /// content preview or authentication material.
    pub analytics_token_hmac_key: Option<AnalyticsTokenHmacKey>,
    pub guest_raw_retention_days: i64,
    pub guest_aggregate_retention_months: i64,
    pub product_import_mapping_version: String,
    pub approved_product_master: Option<ApprovedProductMaster>,
    /// Deployment-owned vocabularies for UTM dimensions. Public requests may
    /// only persist values present here, so analytics endpoints never become
    /// arbitrary free-text storage for names or form content.
    pub analytics_utm_source_allowlist: BTreeSet<String>,
    pub analytics_utm_medium_allowlist: BTreeSet<String>,
    pub analytics_utm_campaign_allowlist: BTreeSet<String>,
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
            .field(
                "invitation_replay_encryption_key",
                &self.invitation_replay_encryption_key,
            )
            .field(
                "product_staging_encryption_key",
                &self.product_staging_encryption_key,
            )
            .field("analytics_token_hmac_key", &self.analytics_token_hmac_key)
            .field("guest_raw_retention_days", &self.guest_raw_retention_days)
            .field(
                "guest_aggregate_retention_months",
                &self.guest_aggregate_retention_months,
            )
            .field(
                "product_import_mapping_version",
                &self.product_import_mapping_version,
            )
            .field("approved_product_master", &self.approved_product_master)
            .field(
                "analytics_utm_source_allowlist_count",
                &self.analytics_utm_source_allowlist.len(),
            )
            .field(
                "analytics_utm_medium_allowlist_count",
                &self.analytics_utm_medium_allowlist.len(),
            )
            .field(
                "analytics_utm_campaign_allowlist_count",
                &self.analytics_utm_campaign_allowlist.len(),
            )
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

#[derive(Clone)]
pub struct ProductStagingEncryptionKey([u8; 32]);

impl ProductStagingEncryptionKey {
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ProductStagingEncryptionKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

#[derive(Clone)]
pub struct AnalyticsTokenHmacKey([u8; 32]);

impl AnalyticsTokenHmacKey {
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for AnalyticsTokenHmacKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

impl fmt::Debug for PreviewSigningKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}

#[derive(Clone)]
pub struct InvitationReplayEncryptionKey([u8; 32]);

impl InvitationReplayEncryptionKey {
    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for InvitationReplayEncryptionKey {
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
    #[error("AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY must be Base64 for exactly 32 bytes")]
    InvalidInvitationReplayEncryptionKey,
    #[error("AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY is required in production builds")]
    MissingInvitationReplayEncryptionKey,
    #[error("AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY must be Base64 for exactly 32 bytes")]
    InvalidProductStagingEncryptionKey,
    #[error("AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY is required in production builds")]
    MissingProductStagingEncryptionKey,
    #[error("AIRTEK_ANALYTICS_TOKEN_HMAC_KEY must be Base64 for exactly 32 bytes")]
    InvalidAnalyticsTokenHmacKey,
    #[error("AIRTEK_ANALYTICS_TOKEN_HMAC_KEY is required in production builds")]
    MissingAnalyticsTokenHmacKey,
    #[error("DATABASE_URL is required in production builds")]
    MissingProductionDatabase,
    #[error("{0} must be an integer in the supported range")]
    InvalidIntegerSetting(&'static str),
    #[error("AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION must not be empty")]
    InvalidProductImportMappingVersion,
    #[error("all AIRTEK_APPROVED_PRODUCT_MASTER_* values must be configured together")]
    IncompleteApprovedProductMaster,
    #[error("AIRTEK_APPROVED_PRODUCT_MASTER_SHA256 must be exactly 64 hexadecimal characters")]
    InvalidApprovedProductMasterSha256,
    #[error("AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION must contain 1 to 100 characters")]
    InvalidApprovedProductMasterMappingVersion,
    #[error("{0} must be an integer between 0 and 50000")]
    InvalidApprovedProductMasterCount(&'static str),
    #[error(
        "the approved Product Master mapping must equal AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION"
    )]
    ApprovedProductMasterMappingMismatch,
    #[error("the AIRTEK_APPROVED_PRODUCT_MASTER_* authority registration is required in production builds")]
    MissingApprovedProductMaster,
    #[error("{0} must be a comma-separated list of non-PII analytics identifiers")]
    InvalidAnalyticsDimensionAllowlist(&'static str),
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
        let invitation_replay_encryption_key =
            non_empty_env("AIRTEK_INVITATION_REPLAY_ENCRYPTION_KEY")
                .map(|value| parse_invitation_replay_encryption_key(&value))
                .transpose()?;
        let product_staging_encryption_key = non_empty_env("AIRTEK_PRODUCT_STAGING_ENCRYPTION_KEY")
            .map(|value| parse_product_staging_encryption_key(&value))
            .transpose()?;
        let analytics_token_hmac_key = non_empty_env("AIRTEK_ANALYTICS_TOKEN_HMAC_KEY")
            .map(|value| parse_analytics_token_hmac_key(&value))
            .transpose()?;
        let guest_raw_retention_days =
            integer_env("AIRTEK_GUEST_RAW_RETENTION_DAYS", 180, 1, 3_650)?;
        let guest_aggregate_retention_months =
            integer_env("AIRTEK_GUEST_AGGREGATE_RETENTION_MONTHS", 24, 1, 120)?;
        let product_import_mapping_version = env::var("AIRTEK_PRODUCT_IMPORT_MAPPING_VERSION")
            .unwrap_or_else(|_| "airtek-basic-v1".into())
            .trim()
            .to_owned();
        if product_import_mapping_version.is_empty() || product_import_mapping_version.len() > 100 {
            return Err(ConfigError::InvalidProductImportMappingVersion);
        }
        let approved_product_master = parse_approved_product_master(
            production,
            non_empty_env("AIRTEK_APPROVED_PRODUCT_MASTER_SHA256"),
            non_empty_env("AIRTEK_APPROVED_PRODUCT_MASTER_MAPPING_VERSION"),
            non_empty_env("AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS"),
            non_empty_env("AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS"),
        )?;
        if production
            && approved_product_master
                .as_ref()
                .is_some_and(|approved| approved.mapping_version != product_import_mapping_version)
        {
            return Err(ConfigError::ApprovedProductMasterMappingMismatch);
        }
        let analytics_utm_source_allowlist =
            analytics_dimension_env("AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES", 128)?;
        let analytics_utm_medium_allowlist =
            analytics_dimension_env("AIRTEK_ANALYTICS_ALLOWED_UTM_MEDIUMS", 128)?;
        let analytics_utm_campaign_allowlist =
            analytics_dimension_env("AIRTEK_ANALYTICS_ALLOWED_UTM_CAMPAIGNS", 200)?;
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
        require_production_database(production, database_url.as_deref())?;
        require_production_keys(
            production,
            totp_encryption_key.as_ref(),
            preview_signing_key.as_ref(),
            invitation_replay_encryption_key.as_ref(),
            product_staging_encryption_key.as_ref(),
            analytics_token_hmac_key.as_ref(),
        )?;

        Ok(Self {
            host,
            public_origin,
            admin_origin,
            database_url,
            admin_bootstrap_token,
            totp_encryption_key,
            preview_signing_key,
            invitation_replay_encryption_key,
            product_staging_encryption_key,
            analytics_token_hmac_key,
            guest_raw_retention_days,
            guest_aggregate_retention_months,
            product_import_mapping_version,
            approved_product_master,
            analytics_utm_source_allowlist,
            analytics_utm_medium_allowlist,
            analytics_utm_campaign_allowlist,
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
            invitation_replay_encryption_key: Some(InvitationReplayEncryptionKey([0x34; 32])),
            product_staging_encryption_key: Some(ProductStagingEncryptionKey([0x54; 32])),
            analytics_token_hmac_key: Some(AnalyticsTokenHmacKey([0x64; 32])),
            guest_raw_retention_days: 180,
            guest_aggregate_retention_months: 24,
            product_import_mapping_version: "airtek-basic-v1".into(),
            approved_product_master: None,
            analytics_utm_source_allowlist: ["google", "google-ads", "e2e-source"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            analytics_utm_medium_allowlist: ["cpc", "integration-test"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            analytics_utm_campaign_allowlist: ["autumn-launch-2026", "admin-acceptance"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
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

fn parse_invitation_replay_encryption_key(
    value: &str,
) -> Result<InvitationReplayEncryptionKey, ConfigError> {
    decode_32_byte_key(value)
        .map(InvitationReplayEncryptionKey)
        .map_err(|_| ConfigError::InvalidInvitationReplayEncryptionKey)
}

fn parse_product_staging_encryption_key(
    value: &str,
) -> Result<ProductStagingEncryptionKey, ConfigError> {
    decode_32_byte_key(value)
        .map(ProductStagingEncryptionKey)
        .map_err(|_| ConfigError::InvalidProductStagingEncryptionKey)
}

fn parse_analytics_token_hmac_key(value: &str) -> Result<AnalyticsTokenHmacKey, ConfigError> {
    decode_32_byte_key(value)
        .map(AnalyticsTokenHmacKey)
        .map_err(|_| ConfigError::InvalidAnalyticsTokenHmacKey)
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
    invitation_replay_encryption_key: Option<&InvitationReplayEncryptionKey>,
    product_staging_encryption_key: Option<&ProductStagingEncryptionKey>,
    analytics_token_hmac_key: Option<&AnalyticsTokenHmacKey>,
) -> Result<(), ConfigError> {
    if production && totp_encryption_key.is_none() {
        return Err(ConfigError::MissingTotpEncryptionKey);
    }
    if production && preview_signing_key.is_none() {
        return Err(ConfigError::MissingPreviewSigningKey);
    }
    if production && invitation_replay_encryption_key.is_none() {
        return Err(ConfigError::MissingInvitationReplayEncryptionKey);
    }
    if production && product_staging_encryption_key.is_none() {
        return Err(ConfigError::MissingProductStagingEncryptionKey);
    }
    if production && analytics_token_hmac_key.is_none() {
        return Err(ConfigError::MissingAnalyticsTokenHmacKey);
    }
    Ok(())
}

fn require_production_database(
    production: bool,
    database_url: Option<&str>,
) -> Result<(), ConfigError> {
    if production && database_url.is_none() {
        Err(ConfigError::MissingProductionDatabase)
    } else {
        Ok(())
    }
}

fn integer_env(
    name: &'static str,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, ConfigError> {
    match non_empty_env(name) {
        None => Ok(default),
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|value| (minimum..=maximum).contains(value))
            .ok_or(ConfigError::InvalidIntegerSetting(name)),
    }
}

fn parse_approved_product_master(
    production: bool,
    sha256: Option<String>,
    mapping_version: Option<String>,
    valid_rows: Option<String>,
    error_rows: Option<String>,
) -> Result<Option<ApprovedProductMaster>, ConfigError> {
    let supplied = [
        sha256.is_some(),
        mapping_version.is_some(),
        valid_rows.is_some(),
        error_rows.is_some(),
    ];
    if supplied.iter().all(|configured| !configured) {
        return if production {
            Err(ConfigError::MissingApprovedProductMaster)
        } else {
            Ok(None)
        };
    }
    if !supplied.iter().all(|configured| *configured) {
        return Err(ConfigError::IncompleteApprovedProductMaster);
    }
    let sha256 = sha256
        .expect("all authority values were checked")
        .to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ConfigError::InvalidApprovedProductMasterSha256);
    }
    let mapping_version = mapping_version.expect("all authority values were checked");
    if mapping_version.is_empty() || mapping_version.len() > 100 {
        return Err(ConfigError::InvalidApprovedProductMasterMappingVersion);
    }
    let expected_valid_rows = approved_product_master_count(
        "AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS",
        valid_rows.expect("all authority values were checked"),
    )?;
    let expected_error_rows = approved_product_master_count(
        "AIRTEK_APPROVED_PRODUCT_MASTER_ERROR_ROWS",
        error_rows.expect("all authority values were checked"),
    )?;
    if production && expected_valid_rows == 0 {
        return Err(ConfigError::InvalidApprovedProductMasterCount(
            "AIRTEK_APPROVED_PRODUCT_MASTER_VALID_ROWS",
        ));
    }
    Ok(Some(ApprovedProductMaster {
        sha256,
        mapping_version,
        expected_valid_rows,
        expected_error_rows,
    }))
}

fn approved_product_master_count(name: &'static str, value: String) -> Result<i64, ConfigError> {
    value
        .parse::<i64>()
        .ok()
        .filter(|value| (0..=50_000).contains(value))
        .ok_or(ConfigError::InvalidApprovedProductMasterCount(name))
}

fn analytics_dimension_env(
    name: &'static str,
    maximum_length: usize,
) -> Result<BTreeSet<String>, ConfigError> {
    let Some(value) = non_empty_env(name) else {
        return Ok(BTreeSet::new());
    };
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let normalized = value.to_ascii_lowercase();
            valid_analytics_dimension_identifier(&normalized, maximum_length)
                .then_some(normalized)
                .ok_or(ConfigError::InvalidAnalyticsDimensionAllowlist(name))
        })
        .collect()
}

fn valid_analytics_dimension_identifier(value: &str, maximum_length: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_length
        && value.is_ascii()
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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
    fn analytics_dimension_allowlists_are_normalized_and_reject_free_text() {
        let values = " Google, linkedin-paid ,newsletter_2026 ";
        let parsed = values
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                let normalized = value.to_ascii_lowercase();
                valid_analytics_dimension_identifier(&normalized, 128)
                    .then_some(normalized)
                    .ok_or(ConfigError::InvalidAnalyticsDimensionAllowlist(
                        "AIRTEK_ANALYTICS_ALLOWED_UTM_SOURCES",
                    ))
            })
            .collect::<Result<BTreeSet<_>, _>>()
            .expect("identifier-only allowlist");
        assert_eq!(
            parsed,
            ["google", "linkedin-paid", "newsletter_2026"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
        for unsafe_value in ["Jane Doe", "person@example.com", "value/with/path", "-bad"] {
            assert!(!valid_analytics_dimension_identifier(unsafe_value, 128));
        }
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
            require_production_keys(true, Some(&totp_key), None, None, None, None),
            Err(ConfigError::MissingPreviewSigningKey)
        ));
        assert!(require_production_keys(false, Some(&totp_key), None, None, None, None).is_ok());
    }

    #[test]
    fn product_analytics_and_invitation_replay_keys_are_independent_and_redacted() {
        let encoded = general_purpose::STANDARD.encode([11_u8; 32]);
        let product = parse_product_staging_encryption_key(&encoded).expect("valid key");
        let analytics = parse_analytics_token_hmac_key(&encoded).expect("valid key");
        let invitation =
            parse_invitation_replay_encryption_key(&encoded).expect("valid invitation replay key");
        assert_eq!(product.as_bytes(), &[11_u8; 32]);
        assert_eq!(analytics.as_bytes(), &[11_u8; 32]);
        assert_eq!(invitation.as_bytes(), &[11_u8; 32]);
        assert_eq!(format!("{product:?}"), "[redacted]");
        assert_eq!(format!("{analytics:?}"), "[redacted]");
        assert_eq!(format!("{invitation:?}"), "[redacted]");
        assert!(parse_invitation_replay_encryption_key("too-short").is_err());
    }

    #[test]
    fn production_requires_product_staging_and_analytics_keys() {
        let totp = TotpEncryptionKey([1; 32]);
        let preview = PreviewSigningKey([2; 32]);
        let invitation = InvitationReplayEncryptionKey([5; 32]);
        let product = ProductStagingEncryptionKey([3; 32]);
        let analytics = AnalyticsTokenHmacKey([4; 32]);
        assert!(matches!(
            require_production_keys(
                true,
                Some(&totp),
                Some(&preview),
                Some(&invitation),
                None,
                Some(&analytics)
            ),
            Err(ConfigError::MissingProductStagingEncryptionKey)
        ));
        assert!(matches!(
            require_production_keys(
                true,
                Some(&totp),
                Some(&preview),
                Some(&invitation),
                Some(&product),
                None
            ),
            Err(ConfigError::MissingAnalyticsTokenHmacKey)
        ));
        assert!(matches!(
            require_production_keys(
                true,
                Some(&totp),
                Some(&preview),
                None,
                Some(&product),
                Some(&analytics)
            ),
            Err(ConfigError::MissingInvitationReplayEncryptionKey)
        ));
        assert!(require_production_keys(
            true,
            Some(&totp),
            Some(&preview),
            Some(&invitation),
            Some(&product),
            Some(&analytics)
        )
        .is_ok());
    }

    #[test]
    fn production_requires_postgresql_configuration() {
        assert!(matches!(
            require_production_database(true, None),
            Err(ConfigError::MissingProductionDatabase)
        ));
        assert!(require_production_database(true, Some("postgres://configured")).is_ok());
        assert!(require_production_database(false, None).is_ok());
    }

    #[test]
    fn production_requires_a_complete_exact_product_master_authority() {
        assert!(matches!(
            parse_approved_product_master(true, None, None, None, None),
            Err(ConfigError::MissingApprovedProductMaster)
        ));
        assert!(matches!(
            parse_approved_product_master(
                true,
                Some("a".repeat(64)),
                Some("airtek-basic-v1".into()),
                Some("370".into()),
                None,
            ),
            Err(ConfigError::IncompleteApprovedProductMaster)
        ));
        assert!(matches!(
            parse_approved_product_master(
                true,
                Some("not-a-sha".into()),
                Some("airtek-basic-v1".into()),
                Some("370".into()),
                Some("5".into()),
            ),
            Err(ConfigError::InvalidApprovedProductMasterSha256)
        ));
        let authority = parse_approved_product_master(
            true,
            Some("A".repeat(64)),
            Some("airtek-basic-v1".into()),
            Some("370".into()),
            Some("5".into()),
        )
        .expect("complete authority")
        .expect("configured authority");
        assert_eq!(authority.sha256, "a".repeat(64));
        assert_eq!(authority.expected_valid_rows, 370);
        assert_eq!(authority.expected_error_rows, 5);
    }

    #[test]
    fn development_may_start_without_product_master_authority() {
        assert_eq!(
            parse_approved_product_master(false, None, None, None, None).unwrap(),
            None
        );
    }
}
