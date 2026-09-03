fn content_fixtures() -> Vec<ContentFixture> {
    let mut fixtures = vec![
        ContentFixture {
            fixture_key: "development/content/home/en",
            ledger_entity_type: "content",
            id: fixture_id(0x10),
            kind: "home",
            slug: "index",
            title: "AIRTEKPOWER Development Preview",
            summary: "Database-backed public website development preview.",
            canonical_path: Some("/en"),
            page_slots: json!({
                "templateKey": "home",
                "eyebrow": "Development fixture",
                "hero": {
                    "eyebrow": "Development fixture",
                    "title": "AIRTEKPOWER Development Preview",
                    "description": "This placeholder verifies the public SSR content pipeline without publishing unreviewed product or business claims."
                },
                "primaryCta": {
                    "eyebrow": "Development fixture",
                    "title": "Prepare a structured inquiry",
                    "description": "Use the RFQ flow to test the development experience.",
                    "label": "Request a Quote",
                    "href": "/en/request-a-quote"
                },
                "sections": [{
                    "id": "database-backed",
                    "eyebrow": "Development fixture",
                    "title": "Database-backed content",
                    "description": "This section is loaded from PostgreSQL and is intentionally non-indexable."
                }]
            }),
            news: None,
        },
        ContentFixture {
            fixture_key: "development/content/news-index/en",
            ledger_entity_type: "content",
            id: fixture_id(0x11),
            // The News landing page is ordinary page content. Only individual
            // News records use the revision-metadata extension and its dedicated
            // Admin API.
            kind: "home",
            slug: "news-index",
            title: "News Development Preview",
            summary: "Development fixtures for the database-backed News experience.",
            canonical_path: Some("/en/resources/news"),
            page_slots: json!({
                "templateKey": "newsIndex",
                "eyebrow": "Development fixture",
                "hero": {
                    "eyebrow": "Development fixture",
                    "title": "News Development Preview",
                    "description": "Six non-indexable placeholder records exercise the News list and detail views."
                },
                "breadcrumbs": [
                    { "label": "Home", "href": "/en" },
                    { "label": "News" }
                ]
            }),
            news: None,
        },
        shell_fixture(
            0x12,
            "development/content/navigation/en",
            "navigation",
            "primary",
            "Primary navigation",
            json!({
                "items": [
                    { "label": "Home", "href": "/en" },
                    { "label": "Products", "href": "/en/products" },
                    { "label": "Solutions", "href": "/en/solutions" },
                    { "label": "Technology", "href": "/en/technology" },
                    { "label": "Resources", "href": "/en/resources/articles" },
                    { "label": "Company", "href": "/en/company/about" },
                    { "label": "News", "href": "/en/resources/news" }
                ]
            }),
        ),
        shell_fixture(
            0x13,
            "development/content/footer/en",
            "footer",
            "primary",
            "Primary footer",
            json!({
                "columns": [
                    {
                        "title": "Explore",
                        "links": [
                            { "label": "Products", "href": "/en/products" },
                            { "label": "Solutions", "href": "/en/solutions" },
                            { "label": "Technology", "href": "/en/technology" }
                        ]
                    },
                    {
                        "title": "Resources",
                        "links": [
                            { "label": "Articles", "href": "/en/resources/articles" },
                            { "label": "News", "href": "/en/resources/news" },
                            { "label": "Downloads", "href": "/en/resources/downloads" }
                        ]
                    },
                    {
                        "title": "Company",
                        "links": [
                            { "label": "About", "href": "/en/company/about" },
                            { "label": "Contact", "href": "/en/company/contact" }
                        ]
                    }
                ],
                "legalLinks": [
                    { "label": "Privacy", "href": "/en/privacy" },
                    { "label": "Terms", "href": "/en/terms" },
                    { "label": "Cookie Settings", "href": "/en/cookie-settings" }
                ]
            }),
        ),
    ];
    fixtures.extend(public_page_fixtures());

    let news = [
        (
            0x20,
            "selection-brief-development-preview",
            "Development Preview: Preparing a Selection Brief",
            "A placeholder record for testing structured engineering-content layouts.",
            "Selection workflow",
            "2026-08-27T08:00:00Z",
        ),
        (
            0x21,
            "operating-point-development-preview",
            "Development Preview: Describing an Operating Point",
            "A placeholder record for testing technical terminology and content relationships.",
            "Engineering basics",
            "2026-08-26T08:00:00Z",
        ),
        (
            0x22,
            "comparison-development-preview",
            "Development Preview: Building a Comparison Checklist",
            "A placeholder record for testing comparison-oriented content cards.",
            "Product discovery",
            "2026-08-25T08:00:00Z",
        ),
        (
            0x23,
            "product-data-development-preview",
            "Development Preview: Reviewing Product Data",
            "A placeholder record for testing product-data governance messaging.",
            "Data quality",
            "2026-08-24T08:00:00Z",
        ),
        (
            0x24,
            "units-development-preview",
            "Development Preview: Recording Units and Conditions",
            "A placeholder record for testing evidence-led editorial structures.",
            "Engineering basics",
            "2026-08-23T08:00:00Z",
        ),
        (
            0x25,
            "rfq-development-preview",
            "Development Preview: From Discovery to RFQ",
            "A placeholder record for testing the content-to-inquiry journey.",
            "RFQ workflow",
            "2026-08-22T08:00:00Z",
        ),
    ];
    fixtures.extend(news.into_iter().map(
        |(suffix, slug, title, summary, category, publication_at)| ContentFixture {
            fixture_key: match suffix {
                0x20 => "development/news/selection-brief/en",
                0x21 => "development/news/operating-point/en",
                0x22 => "development/news/comparison/en",
                0x23 => "development/news/product-data/en",
                0x24 => "development/news/units/en",
                _ => "development/news/rfq/en",
            },
            ledger_entity_type: "news",
            id: fixture_id(suffix),
            kind: "news",
            slug,
            title,
            summary,
            canonical_path: Some(match suffix {
                0x20 => "/en/resources/news/selection-brief-development-preview",
                0x21 => "/en/resources/news/operating-point-development-preview",
                0x22 => "/en/resources/news/comparison-development-preview",
                0x23 => "/en/resources/news/product-data-development-preview",
                0x24 => "/en/resources/news/units-development-preview",
                _ => "/en/resources/news/rfq-development-preview",
            }),
            page_slots: json!({
                "templateKey": "newsDetail",
                "eyebrow": "Development fixture",
                "hero": {
                    "eyebrow": category,
                    "title": title,
                    "description": summary
                },
                "breadcrumbs": [
                    { "label": "Home", "href": "/en" },
                    { "label": "News", "href": "/en/resources/news" },
                    { "label": title }
                ],
                "sections": [{
                    "id": "fixture-notice",
                    "eyebrow": "Development fixture",
                    "title": "Placeholder content",
                    "description": "This record contains no approved product specification or publication claim."
                }],
                "primaryCta": {
                    "eyebrow": "Next step",
                    "title": "Continue with a structured inquiry",
                    "description": "Use the development RFQ flow without treating this fixture as engineering advice.",
                    "label": "Request a Quote",
                    "href": "/en/request-a-quote"
                }
            }),
            news: Some(NewsFixture {
                category,
                publication_at,
            }),
        },
    ));
    fixtures
}
