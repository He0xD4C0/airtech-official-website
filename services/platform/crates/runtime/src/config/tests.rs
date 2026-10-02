#[cfg(test)]
mod cases {
    use super::super::*;

    #[test]
    pub(super) fn trusted_proxy_cidrs_support_ipv4_ipv6_and_exact_addresses() {
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
    pub(super) fn invalid_trusted_proxy_cidr_is_rejected() {
        assert!(matches!(
            parse_trusted_proxy_cidrs("172.28.0.0/64"),
            Err(ConfigError::InvalidTrustedProxyCidr(_))
        ));
    }

    #[test]
    pub(super) fn analytics_dimension_allowlists_are_normalized_and_reject_free_text() {
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
    pub(super) fn totp_key_requires_exactly_32_decoded_bytes_and_is_redacted() {
        let encoded = general_purpose::STANDARD.encode([7_u8; 32]);
        let key = parse_totp_encryption_key(&encoded).expect("valid key");
        assert_eq!(key.as_bytes(), &[7_u8; 32]);
        assert_eq!(format!("{key:?}"), "[redacted]");
        assert!(parse_totp_encryption_key("too-short").is_err());
    }

    #[test]
    pub(super) fn application_keys_are_independent_and_redacted() {
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
    pub(super) fn production_requires_product_staging_and_analytics_keys() {
        let invitation = InvitationReplayEncryptionKey([5; 32]);
        let product = ProductStagingEncryptionKey([3; 32]);
        let analytics = AnalyticsTokenHmacKey([4; 32]);
        assert!(matches!(
            require_production_keys(true, Some(&invitation), None, Some(&analytics)),
            Err(ConfigError::MissingProductStagingEncryptionKey)
        ));
        assert!(matches!(
            require_production_keys(true, Some(&invitation), Some(&product), None),
            Err(ConfigError::MissingAnalyticsTokenHmacKey)
        ));
        assert!(matches!(
            require_production_keys(true, None, Some(&product), Some(&analytics)),
            Err(ConfigError::MissingInvitationReplayEncryptionKey)
        ));
        assert!(
            require_production_keys(true, Some(&invitation), Some(&product), Some(&analytics))
                .is_ok()
        );
    }

    #[test]
    pub(super) fn production_requires_postgresql_configuration() {
        assert!(matches!(
            require_production_database(true, None),
            Err(ConfigError::MissingProductionDatabase)
        ));
        assert!(require_production_database(true, Some("postgres://configured")).is_ok());
        assert!(require_production_database(false, None).is_ok());
    }

    #[test]
    pub(super) fn production_requires_a_complete_exact_product_master_authority() {
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
    pub(super) fn development_may_start_without_product_master_authority() {
        assert_eq!(
            parse_approved_product_master(false, None, None, None, None).unwrap(),
            None
        );
    }

    #[cfg(feature = "devtools")]
    #[test]
    pub(super) fn password_only_development_auth_is_limited_to_the_fixed_local_admin() {
        let mut config = Config::for_test();
        config.development_admin_password_only = true;
        assert!(config.development_password_only_for(DEVELOPMENT_ADMIN_EMAIL));
        assert!(config.development_password_only_for("LOCAL-ADMIN@AIRTEK.INVALID"));
        assert!(!config.development_password_only_for("codex-qa@airtek.invalid"));
        assert!(!config.development_password_only_for("admin@example.com"));
    }
}
