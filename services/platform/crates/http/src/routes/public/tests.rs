#[cfg(test)]
mod cases {
    use super::super::*;
    use serde_json::json;

    pub(super) fn analytics_event(event_name: &str, source_path: &str) -> CreateAnalyticsEvent {
        CreateAnalyticsEvent {
            event_name: event_name.into(),
            anonymous_session_id: Some(Uuid::new_v4()),
            source_path: source_path.into(),
            locale: "en".into(),
            consent_granted: true,
            policy_version: Some(ANALYTICS_POLICY_VERSION.into()),
            consent_receipt: Some(Uuid::new_v4()),
            properties: BTreeMap::new(),
        }
    }

    #[test]
    pub(super) fn analytics_source_path_rejects_direct_and_encoded_identifiers() {
        for source_path in [
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
            assert!(
                validate_analytics_event(&analytics_event("pageView", source_path)).is_err(),
                "source path {source_path:?} must be rejected"
            );
        }
    }

    #[test]
    pub(super) fn analytics_accepts_only_controlled_cta_dimensions() {
        let mut event = analytics_event("ctaClicked", "/en/products");
        event.properties = BTreeMap::from([
            ("ctaId".into(), json!("request-quote")),
            ("placement".into(), json!("product-detail")),
        ]);
        validate_analytics_event(&event).expect("controlled CTA dimensions are valid");
    }

    #[test]
    pub(super) fn analytics_controlled_dimensions_reject_arbitrary_identifiers() {
        for (event_name, properties) in [
            (
                "pageView",
                BTreeMap::from([("contentKind".into(), json!("Jane Doe"))]),
            ),
            (
                "filterApplied",
                BTreeMap::from([("filterName".into(), json!("jane-doe"))]),
            ),
            (
                "faqExpanded",
                BTreeMap::from([("faqId".into(), json!("jane-doe"))]),
            ),
            (
                "ctaClicked",
                BTreeMap::from([
                    ("ctaId".into(), json!("private-note")),
                    ("placement".into(), json!("hero")),
                ]),
            ),
            (
                "ctaClicked",
                BTreeMap::from([
                    ("ctaId".into(), json!("request-quote")),
                    ("placement".into(), json!("jane-doe")),
                ]),
            ),
            (
                "rfqValidationError",
                BTreeMap::from([
                    ("journey".into(), json!("product")),
                    ("step".into(), json!(1)),
                    ("fieldName".into(), json!("jane-doe")),
                    ("errorCode".into(), json!("publishedContextRequired")),
                ]),
            ),
            (
                "rfqSubmitFailed",
                BTreeMap::from([
                    ("journey".into(), json!("product")),
                    ("errorCode".into(), json!("jane-doe")),
                ]),
            ),
        ] {
            let mut event = analytics_event(event_name, "/en/products");
            event.properties = properties;
            assert!(
                validate_analytics_event(&event).is_err(),
                "{event_name} must reject arbitrary controlled-dimension values"
            );
        }

        let mut removed_destination = analytics_event("ctaClicked", "/en/products");
        removed_destination.properties = BTreeMap::from([
            ("ctaId".into(), json!("request-quote")),
            ("placement".into(), json!("hero")),
            ("destinationPath".into(), json!("/en/jane-doe")),
        ]);
        assert!(validate_analytics_event(&removed_destination).is_err());
    }

    #[test]
    pub(super) fn product_detail_lookup_never_selects_an_arbitrary_slug_collision() {
        let error = require_unique_published_product(vec!["axial", "centrifugal"])
            .expect_err("a cross-family slug collision must be disambiguated");
        assert_eq!(error.status(), StatusCode::CONFLICT);
        assert_eq!(
            require_unique_published_product(vec!["axial"])
                .expect("a family-bound lookup is unique"),
            "axial"
        );
    }
}
