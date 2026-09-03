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
