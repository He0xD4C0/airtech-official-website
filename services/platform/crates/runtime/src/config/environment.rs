use super::*;

pub fn reject_development_seed_configuration() -> Result<(), ConfigError> {
    if !cfg!(feature = "devtools") {
        for name in ["AIRTEK_DEV_PUBLIC_SEED"] {
            if env::var_os(name).is_some() {
                return Err(ConfigError::DevelopmentSeedForbidden(name));
            }
        }
    }
    Ok(())
}

pub(super) fn parse_totp_encryption_key(value: &str) -> Result<TotpEncryptionKey, ConfigError> {
    decode_32_byte_key(value)
        .map(TotpEncryptionKey)
        .map_err(|_| ConfigError::InvalidTotpEncryptionKey)
}

pub(super) fn parse_admin_recovery_key_mode() -> Result<AdminRecoveryKeyMode, ConfigError> {
    match env::var("AIRTEK_ADMIN_RECOVERY_KEY_MODE")
        .unwrap_or_else(|_| "auto".into())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "auto" => Ok(AdminRecoveryKeyMode::Auto),
        "generate" => Ok(AdminRecoveryKeyMode::Generate),
        "load" => Ok(AdminRecoveryKeyMode::Load),
        _ => Err(ConfigError::InvalidAdminRecoveryKeyMode),
    }
}

pub(super) fn parse_captcha_failure_mode() -> Result<CaptchaFailureMode, ConfigError> {
    match env::var("AIRTEK_CAPTCHA_FAILURE_MODE")
        .unwrap_or_else(|_| "fail-open".into())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "fail-open" => Ok(CaptchaFailureMode::FailOpen),
        "fail-closed" => Ok(CaptchaFailureMode::FailClosed),
        _ => Err(ConfigError::InvalidCaptchaFailureMode),
    }
}

pub(super) fn parse_invitation_replay_encryption_key(
    value: &str,
) -> Result<InvitationReplayEncryptionKey, ConfigError> {
    decode_32_byte_key(value)
        .map(InvitationReplayEncryptionKey)
        .map_err(|_| ConfigError::InvalidInvitationReplayEncryptionKey)
}

pub(super) fn parse_product_staging_encryption_key(
    value: &str,
) -> Result<ProductStagingEncryptionKey, ConfigError> {
    decode_32_byte_key(value)
        .map(ProductStagingEncryptionKey)
        .map_err(|_| ConfigError::InvalidProductStagingEncryptionKey)
}

pub(super) fn parse_analytics_token_hmac_key(
    value: &str,
) -> Result<AnalyticsTokenHmacKey, ConfigError> {
    decode_32_byte_key(value)
        .map(AnalyticsTokenHmacKey)
        .map_err(|_| ConfigError::InvalidAnalyticsTokenHmacKey)
}

pub(super) fn decode_32_byte_key(value: &str) -> Result<[u8; 32], ()> {
    general_purpose::STANDARD
        .decode(value)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(value))
        .map_err(|_| ())?
        .try_into()
        .map_err(|_| ())
}

pub(super) fn require_production_keys(
    production: bool,
    invitation_replay_encryption_key: Option<&InvitationReplayEncryptionKey>,
    product_staging_encryption_key: Option<&ProductStagingEncryptionKey>,
    analytics_token_hmac_key: Option<&AnalyticsTokenHmacKey>,
) -> Result<(), ConfigError> {
    // TOTP is optional, so its encryption key is only needed once an
    // administrator actually enrolls; enrollment returns a clear
    // "unavailable" error while the key is absent.
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

pub(super) fn require_production_database(
    production: bool,
    database_url: Option<&str>,
) -> Result<(), ConfigError> {
    if production && database_url.is_none() {
        Err(ConfigError::MissingProductionDatabase)
    } else {
        Ok(())
    }
}

pub(super) fn integer_env(
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

pub(super) fn parse_approved_product_master(
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

pub(super) fn approved_product_master_count(
    name: &'static str,
    value: String,
) -> Result<i64, ConfigError> {
    value
        .parse::<i64>()
        .ok()
        .filter(|value| (0..=50_000).contains(value))
        .ok_or(ConfigError::InvalidApprovedProductMasterCount(name))
}

pub(super) fn analytics_dimension_env(
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

pub(super) fn valid_analytics_dimension_identifier(value: &str, maximum_length: usize) -> bool {
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

pub(super) fn parse_trusted_proxy_cidrs(value: &str) -> Result<Vec<IpCidr>, ConfigError> {
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

pub(super) fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|v| !v.is_empty())
}

pub(super) fn validate_origin(name: &'static str, value: &str) -> Result<(), ConfigError> {
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
