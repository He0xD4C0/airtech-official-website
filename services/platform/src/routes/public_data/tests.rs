#[cfg(test)]
mod tests {
    use super::*;

    fn site_information(payload: Value, is_placeholder: bool) -> GeneralInformation {
        GeneralInformation {
            id: Uuid::new_v4(),
            locale: "en".into(),
            payload,
            status: PublicationStatus::Published,
            current_revision: 1,
            published_revision: Some(1),
            is_placeholder,
            updated_at: Utc::now(),
        }
    }

    fn shell_content(kind: ContentKind, is_placeholder: bool) -> ContentEntry {
        ContentEntry {
            id: Uuid::new_v4(),
            kind,
            slug: match kind {
                ContentKind::Navigation => "primary-navigation",
                ContentKind::Footer => "primary-footer",
                _ => unreachable!("site shell fixture only supports navigation and footer"),
            }
            .into(),
            locale: "en".into(),
            title: "Published site shell".into(),
            summary: None,
            body: crate::models::RichTextDocument {
                schema_version: 1,
                doc: json!({"type": "doc", "content": []}),
            },
            seo: crate::models::SeoMetadata::default(),
            status: PublicationStatus::Published,
            is_placeholder,
            current_revision: 1,
            published_revision: Some(1),
            scheduled_for: None,
            updated_at: Utc::now(),
        }
    }

    async fn install_site_shell(
        state: &AppState,
        information: GeneralInformation,
        navigation_placeholder: bool,
        footer_placeholder: bool,
    ) {
        let navigation = shell_content(ContentKind::Navigation, navigation_placeholder);
        let footer = shell_content(ContentKind::Footer, footer_placeholder);
        let mut data = state.data.write().await;
        data.published_general_information
            .insert(information.id, information);
        data.published_content.insert(navigation.id, navigation);
        data.published_content.insert(footer.id, footer);
    }

    fn complete_site_information(is_placeholder: bool) -> GeneralInformation {
        site_information(
            json!({
                "brandName": "AIRTEKPOWER",
                "homePath": "/en",
                "organization": {"name": "AIRTEKPOWER"}
            }),
            is_placeholder,
        )
    }

    fn guest_visit_fixture() -> CreateGuestVisit {
        CreateGuestVisit {
            anonymous_session_id: Uuid::new_v4(),
            consent_receipt: Uuid::new_v4(),
            policy_version: super::super::public::ANALYTICS_POLICY_VERSION.into(),
            landing_path: "/en/products/b23e280h128-102-b0".into(),
            referrer_domain: Some("search.example.com".into()),
            source: Some("google".into()),
            medium: Some("cpc".into()),
            campaign: Some("autumn-launch-2026".into()),
        }
    }

    #[test]
    fn guest_visit_rejects_full_referrer_and_tracking_query() {
        let mut request = guest_visit_fixture();
        request.landing_path = "/en/products?email=private@example.com".into();
        request.referrer_domain = Some("https://example.com/a?q=private".into());
        let problem = validate_guest_visit(&Config::for_test(), &request)
            .expect_err("unsafe attribution is rejected");
        assert_eq!(problem.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn guest_visit_normalizes_safe_attribution_before_storage() {
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
    fn guest_visit_rejects_non_hostname_or_identifier_referrers() {
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
    fn guest_visit_rejects_pii_like_attribution_values() {
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
    fn guest_visit_rejects_direct_and_encoded_identifiers_in_landing_path() {
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
    fn guest_visit_accepts_canonical_product_path_and_standard_utm_values() {
        validate_guest_visit(&Config::for_test(), &guest_visit_fixture())
            .expect("canonical paths and non-PII UTM dimensions are accepted");
    }

    #[test]
    fn guest_visit_rejects_names_and_unregistered_free_text_dimensions() {
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
    fn source_classification_is_data_minimized() {
        let request = guest_visit_fixture();
        assert_eq!(classify_source(&request), "paidSearch");
    }

    #[test]
    fn configured_product_family_remains_visible_without_any_products() {
        let information = GeneralInformation {
            id: Uuid::new_v4(),
            locale: "en".into(),
            payload: json!({
                "productCategories": [{
                    "code": "axial",
                    "slug": "axial",
                    "name": "Axial fans",
                    "description": "Configured category with an intentionally empty catalog.",
                    "sortOrder": 2
                }]
            }),
            status: PublicationStatus::Published,
            current_revision: 1,
            published_revision: Some(1),
            is_placeholder: false,
            updated_at: Utc::now(),
        };

        let presentations = product_family_presentations(Some(&information));
        assert_eq!(presentations.len(), 1);
        assert_eq!(presentations[0].code, ProductFamily::Axial);
        assert_eq!(presentations[0].slug, "axial");
    }

    #[tokio::test]
    async fn public_discovery_rejects_a_missing_site_shell() {
        let error = published_site_shell_has_placeholder(&AppState::for_test(), "en")
            .await
            .expect_err("missing site shell must fail closed");
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn public_discovery_rejects_an_incomplete_site_shell() {
        let state = AppState::for_test();
        install_site_shell(&state, site_information(json!({}), false), false, false).await;

        let error = published_site_shell_has_placeholder(&state, "en")
            .await
            .expect_err("incomplete General Information must fail closed");
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn public_discovery_returns_no_entries_for_a_placeholder_site_shell() {
        let state = AppState::for_test();
        install_site_shell(&state, complete_site_information(true), false, false).await;

        assert!(published_site_shell_has_placeholder(&state, "en")
            .await
            .expect("placeholder lookup succeeds"));
    }

    #[tokio::test]
    async fn public_discovery_accepts_a_complete_non_placeholder_site_shell() {
        let state = AppState::for_test();
        install_site_shell(&state, complete_site_information(false), false, false).await;

        assert!(!published_site_shell_has_placeholder(&state, "en")
            .await
            .expect("complete site shell lookup succeeds"));
    }
}
