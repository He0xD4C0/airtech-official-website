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
    #[error("media configuration is invalid: {0}")]
    InvalidMediaSettings(String),
    #[error("a devtools build requires DATABASE_URL or AIRTEK_ADMIN_BOOTSTRAP_TOKEN so an authenticated administrator can be established")]
    MissingDevtoolsAuthentication,
}
