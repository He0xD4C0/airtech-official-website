fn content_definition(fixture: &ContentFixture) -> Value {
    json!({
        "seedVersion": SEED_VERSION,
        "kind": fixture.kind,
        "slug": fixture.slug,
        "locale": "en",
        "title": fixture.title,
        "summary": fixture.summary,
        "canonicalPath": fixture.canonical_path,
        "pageSlots": fixture.page_slots,
        "news": fixture.news.as_ref().map(|news| json!({
            "category": news.category,
            "publicationAt": news.publication_at,
            "authorDisplayName": "AIRTEKPOWER Development Fixture",
            "featured": false,
            "readingMinutes": 2
        })),
        "isPlaceholder": true,
        "indexable": false,
        "dataOrigin": "developmentFixture"
    })
}

fn content_payload(fixture: &ContentFixture, revision: i64) -> Result<Value, DevelopmentSeedError> {
    let description = format!("{} Development-only placeholder content.", fixture.summary);
    Ok(json!({
        "id": fixture.id,
        "kind": fixture.kind,
        "slug": fixture.slug,
        "locale": "en",
        "title": fixture.title,
        "summary": fixture.summary,
        "body": {
            "schemaVersion": 1,
            "doc": {
                "type": "doc",
                "attrs": { "pageSlots": fixture.page_slots },
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "This development fixture verifies database-backed rendering. Replace it with reviewed editorial content before launch."
                    }]
                }]
            }
        },
        "seo": {
            "title": format!("{} | Development Preview", fixture.title),
            "description": description,
            "canonicalPath": fixture.canonical_path,
            "indexable": false
        },
        "status": "published",
        "isPlaceholder": true,
        "currentRevision": revision,
        "publishedRevision": revision,
        "scheduledFor": null,
        "updatedAt": FIXTURE_UPDATED_AT
    }))
}

fn general_information_payload() -> Value {
    json!({
        "brandName": "AIRTEKPOWER",
        "brandLine": "Redefining Airflow with Smart, Green Technology",
        "homePath": "/en",
        "footerStatement": "Development fixture content. Replace all placeholders with reviewed, source-backed publication data before launch.",
        "copyrightText": "© {year} AIRTEKPOWER. Development fixture.",
        "defaultSeo": {
            "title": "AIRTEKPOWER | Development Preview",
            "description": "A database-backed development preview for the AIRTEKPOWER public website."
        },
        "organization": {
            "name": "AIRTEKPOWER"
        },
        "navigationCta": {
            "label": "Request a Quote",
            "href": "/en/request-a-quote"
        },
        "productCategories": product_category_presentations()
    })
}

fn product_category_presentations() -> Value {
    json!([
        {
            "code": "centrifugal",
            "slug": "centrifugal",
            "name": "Centrifugal",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 10
        },
        {
            "code": "axial",
            "slug": "axial",
            "name": "Axial",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 20
        },
        {
            "code": "crossFlow",
            "slug": "cross-flow",
            "name": "Cross-flow",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 30
        },
        {
            "code": "inlineDuct",
            "slug": "inline-duct",
            "name": "Inline Duct",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 40
        },
        {
            "code": "motors",
            "slug": "motors",
            "name": "Motors",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 50
        }
    ])
}
