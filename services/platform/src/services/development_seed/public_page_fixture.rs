#[allow(clippy::too_many_arguments)]
fn public_page_fixture(
    suffix: u128,
    fixture_key: &'static str,
    kind: &'static str,
    slug: &'static str,
    title: &'static str,
    canonical_path: &'static str,
    template_key: &'static str,
    eyebrow: &'static str,
    description: &'static str,
) -> ContentFixture {
    ContentFixture {
        fixture_key,
        ledger_entity_type: "content",
        id: fixture_id(suffix),
        kind,
        slug,
        title,
        summary: description,
        canonical_path: Some(canonical_path),
        page_slots: json!({
            "templateKey": template_key,
            "eyebrow": eyebrow,
            "hero": {
                "eyebrow": eyebrow,
                "title": title,
                "description": description
            },
            "breadcrumbs": [
                { "label": "Home", "href": "/en" },
                { "label": title }
            ],
            "sections": [{
                "id": "database-placeholder",
                "eyebrow": "Development fixture",
                "title": "Database-backed placeholder",
                "description": "This page block verifies the SSR and admin editing pipeline. It is non-indexable and must be replaced before launch."
            }],
            "primaryCta": {
                "eyebrow": "Development fixture",
                "title": "Continue to a structured inquiry",
                "description": "Use the development RFQ workflow without treating placeholder text as product guidance.",
                "label": "Request a Quote",
                "href": "/en/request-a-quote"
            }
        }),
        news: None,
    }
}
