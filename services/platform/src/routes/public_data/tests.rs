#[cfg(test)]
mod cases {
    use super::super::*;

    pub(super) fn guest_visit_fixture() -> CreateGuestVisit {
        CreateGuestVisit {
            anonymous_session_id: Uuid::new_v4(),
            consent_receipt: Uuid::new_v4(),
            policy_version: crate::routes::public::ANALYTICS_POLICY_VERSION.into(),
            landing_path: "/en/products/b23e280h128-102-b0".into(),
            referrer_domain: Some("search.example.com".into()),
            source: Some("google".into()),
            medium: Some("cpc".into()),
            campaign: Some("autumn-launch-2026".into()),
        }
    }

    #[test]
    pub(super) fn guest_visit_rejects_full_referrer_and_tracking_query() {
        let mut request = guest_visit_fixture();
        request.landing_path = "/en/products?email=private@example.com".into();
        request.referrer_domain = Some("https://example.com/a?q=private".into());
        let problem = validate_guest_visit(&Config::for_test(), &request)
            .expect_err("unsafe attribution is rejected");
        assert_eq!(problem.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    pub(super) fn guest_visit_normalizes_safe_attribution_before_storage() {
        let mut request = guest_visit_fixture();
        request.referrer_domain = Some("  Search.Example.COM  ".into());
        request.source = Some("  Google-Ads  ".into());
        request.medium = Some(" cpc ".into());
        request.campaign = Some("   ".into());

        validate_guest_visit(&Config::for_test(), &request).expect("safe attribution is accepted");
        let normalized = normalize_guest_visit(request);

        assert_eq!(
            normalized.referrer_domain.as_deref(),
            Some("search.example.com")
        );
        assert_eq!(normalized.source.as_deref(), Some("google-ads"));
        assert_eq!(normalized.medium.as_deref(), Some("cpc"));
        assert_eq!(normalized.campaign, None);
    }

    #[test]
    pub(super) fn guest_visit_rejects_non_hostname_or_identifier_referrers() {
        for referrer in [
            "192.0.2.10",
            "2001:db8::1",
            "person@example.com",
            "13800138000.example",
            "bad_host.example",
            "-bad.example",
            "bad-.example",
            "bad..example",
            "example.com.",
            "example.com\n",
        ] {
            let mut request = guest_visit_fixture();
            request.referrer_domain = Some(referrer.into());
            assert!(
                validate_guest_visit(&Config::for_test(), &request).is_err(),
                "referrer {referrer:?} must be rejected"
            );
        }
    }

    #[test]
    pub(super) fn guest_visit_rejects_pii_like_attribution_values() {
        for value in [
            "person@example.com",
            "+86 13800138000",
            "call +86 13800138000",
            "192.0.2.10",
            "source 2001:db8::1",
            "token=secret-value",
            "550e8400-e29b-41d4-a716-446655440000",
            "line\nbreak",
        ] {
            for field in ["source", "medium", "campaign"] {
                let mut request = guest_visit_fixture();
                match field {
                    "source" => request.source = Some(value.into()),
                    "medium" => request.medium = Some(value.into()),
                    "campaign" => request.campaign = Some(value.into()),
                    _ => unreachable!(),
                }
                assert!(
                    validate_guest_visit(&Config::for_test(), &request).is_err(),
                    "{field} value {value:?} must be rejected"
                );
            }
        }
    }

    #[test]
    pub(super) fn guest_visit_rejects_direct_and_encoded_identifiers_in_landing_path() {
        for landing_path in [
            "/en/ref/person@example.com",
            "/en/ref/person%40example.com",
            "/en/ref/%31%33%38%30%30%31%33%38%30%30%30",
            "/en/ref/192.0.2.10",
            "/en/ref/550e8400-e29b-41d4-a716-446655440000",
            "/en/ref/%2540",
            "/en/ref/%0aheader",
            "/en/ref\\private",
            "/en/ref/line\nbreak",
        ] {
            let mut request = guest_visit_fixture();
            request.landing_path = landing_path.into();
            assert!(
                validate_guest_visit(&Config::for_test(), &request).is_err(),
                "landing path {landing_path:?} must be rejected"
            );
        }
    }

    #[test]
    pub(super) fn guest_visit_accepts_canonical_product_path_and_standard_utm_values() {
        validate_guest_visit(&Config::for_test(), &guest_visit_fixture())
            .expect("canonical paths and non-PII UTM dimensions are accepted");
    }

    #[test]
    pub(super) fn guest_visit_rejects_names_and_unregistered_free_text_dimensions() {
        for value in ["Jane Doe", "unregistered-campaign", "form-body-value"] {
            for field in ["source", "medium", "campaign"] {
                let mut request = guest_visit_fixture();
                match field {
                    "source" => request.source = Some(value.into()),
                    "medium" => request.medium = Some(value.into()),
                    "campaign" => request.campaign = Some(value.into()),
                    _ => unreachable!(),
                }
                assert!(
                    validate_guest_visit(&Config::for_test(), &request).is_err(),
                    "unregistered {field} value {value:?} must be rejected"
                );
            }
        }
    }

    #[test]
    pub(super) fn source_classification_is_data_minimized() {
        let request = guest_visit_fixture();
        assert_eq!(classify_source(&request), "paidSearch");
    }

    #[test]
    pub(super) fn configured_product_family_remains_visible_without_any_products() {
        let categories = [crate::models::ProductCategoryPresentationInput {
            code: ProductFamily::Axial,
            slug: "axial".into(),
            name: "Axial fans".into(),
            description: "Configured category with an intentionally empty catalog.".into(),
            sort_order: 2,
        }];
        let presentations = product_family_presentations(&categories);
        assert_eq!(presentations.len(), 1);
        assert_eq!(presentations[0].code, ProductFamily::Axial);
        assert_eq!(presentations[0].slug, "axial");
    }
}
