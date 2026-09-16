use super::*;

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
        let media = MediaSettings::from_env().map_err(ConfigError::InvalidMediaSettings)?;
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
            media,
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
            media: MediaSettings::disabled(),
            production: false,
        }
    }

    pub fn bind_address(&self) -> (IpAddr, u16) {
        (self.host, API_PORT)
    }
}
