#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_contract_contains_six_safe_news_fixtures_and_no_products() {
        let fixtures = content_fixtures();
        let news = fixtures
            .iter()
            .filter(|fixture| fixture.news.is_some())
            .collect::<Vec<_>>();
        assert_eq!(news.len(), 6);
        assert!(fixtures
            .iter()
            .all(|fixture| fixture.ledger_entity_type != "product"));
        for fixture in news {
            let payload = content_payload(fixture, 1).unwrap();
            assert_eq!(payload["isPlaceholder"], true);
            assert_eq!(payload["seo"]["indexable"], false);
            assert_eq!(payload["status"], "published");
        }
    }

    #[test]
    fn site_shell_payload_matches_the_public_contract() {
        let information = general_information_payload();
        assert_eq!(information["brandName"], "AIRTEKPOWER");
        assert_eq!(information["homePath"], "/en");
        assert!(information["defaultSeo"].is_object());
        assert!(information["organization"].is_object());
        assert!(information["navigationCta"].is_object());
        assert_eq!(
            information["productCategories"].as_array().map(Vec::len),
            Some(5)
        );

        let fixtures = content_fixtures();
        let navigation = fixtures
            .iter()
            .find(|fixture| fixture.kind == "navigation")
            .unwrap();
        let footer = fixtures
            .iter()
            .find(|fixture| fixture.kind == "footer")
            .unwrap();
        assert!(navigation.page_slots["items"].is_array());
        assert!(footer.page_slots["columns"].is_array());
        assert!(footer.page_slots["legalLinks"].is_array());
    }

    #[test]
    fn every_planned_static_public_route_has_a_page_block() {
        let fixtures = content_fixtures();
        let paths = fixtures
            .iter()
            .filter_map(|fixture| fixture.canonical_path)
            .collect::<std::collections::BTreeSet<_>>();
        for required in [
            "/en",
            "/en/products",
            "/en/products/centrifugal",
            "/en/products/axial",
            "/en/products/cross-flow",
            "/en/products/inline-duct",
            "/en/products/motors",
            "/en/products/selector",
            "/en/products/compare",
            "/en/solutions",
            "/en/solutions/hvac",
            "/en/solutions/refrigeration",
            "/en/solutions/data-centers",
            "/en/solutions/energy-storage",
            "/en/solutions/air-purification",
            "/en/solutions/cleanroom",
            "/en/solutions/industrial-ventilation",
            "/en/solutions/commercial-buildings",
            "/en/technology",
            "/en/technology/ec-motor",
            "/en/technology/aerodynamics",
            "/en/technology/airflow-and-pressure",
            "/en/technology/control",
            "/en/technology/efficiency",
            "/en/technology/noise-and-vibration",
            "/en/resources/articles",
            "/en/resources/news",
            "/en/resources/faqs",
            "/en/resources/case-studies",
            "/en/resources/downloads",
            "/en/company/about",
            "/en/company/contact",
            "/en/request-a-quote",
            "/en/request-a-quote/product",
            "/en/request-a-quote/selection",
            "/en/request-a-quote/project",
            "/en/request-a-quote/replacement",
            "/en/search",
            "/en/privacy",
            "/en/terms",
            "/en/cookie-settings",
        ] {
            assert!(paths.contains(required), "missing public route {required}");
        }
        for fixture in fixtures
            .iter()
            .filter(|fixture| fixture.canonical_path.is_some())
        {
            let payload = content_payload(fixture, 1).unwrap();
            assert!(
                payload["body"]["doc"]["attrs"]["pageSlots"]["templateKey"].is_string(),
                "missing templateKey for {}",
                fixture.fixture_key
            );
            assert_eq!(payload["isPlaceholder"], true);
            assert_eq!(payload["seo"]["indexable"], false);
        }
    }

    #[test]
    fn fixture_definitions_and_ids_are_stable() {
        let fixtures = content_fixtures();
        let keys = fixtures
            .iter()
            .map(|fixture| fixture.fixture_key)
            .collect::<std::collections::BTreeSet<_>>();
        let ids = fixtures
            .iter()
            .map(|fixture| fixture.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(keys.len(), fixtures.len());
        assert_eq!(ids.len(), fixtures.len());
        assert_eq!(
            checksum(&content_definition(&fixtures[0])),
            checksum(&content_definition(&fixtures[0]))
        );
    }
}
