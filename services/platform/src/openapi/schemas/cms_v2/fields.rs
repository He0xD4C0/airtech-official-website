//! Strict type-specific fields and the complete CMS V2 draft shape.

use serde_json::{json, Map, Value};

use super::super::super::support::*;
use super::core::typed_object;

pub(super) fn add(s: &mut Map<String, Value>) {
    add_seo_and_taxonomy(s);
    add_editorial_fields(s);
    add_general_information(s);
    add_navigation_and_footer(s);
    add_type_fields_union(s);
    add_draft(s);
}

fn add_seo_and_taxonomy(s: &mut Map<String, Value>) {
    s.insert(
        "SeoInputV2".into(),
        object(
            &["indexable"],
            json!({
                "title": nullable(json!({"type": "string", "maxLength": 300})),
                "description": nullable(json!({"type": "string", "maxLength": 1000})),
                "indexable": {"type": "boolean"},
                "socialImage": nullable(r("MediaUseReference"))
            }),
        ),
    );
    s.insert(
        "TaxonomyTypeFields".into(),
        object(
            &[],
            json!({"key": nullable(json!({"type": "string", "maxLength": 120}))}),
        ),
    );
}

fn add_editorial_fields(s: &mut Map<String, Value>) {
    s.insert(
        "FaqItem".into(),
        object(
            &["id", "question", "answer"],
            json!({
                "id": uuid(),
                "question": {"type": "string", "maxLength": 500},
                "answer": r("TiptapDocument")
            }),
        ),
    );
    s.insert(
        "EditorialTypeFields".into(),
        object(
            &["featured"],
            json!({
                "category": nullable(json!({"type": "string", "maxLength": 120})),
                "authorDisplayName": nullable(json!({"type": "string", "maxLength": 200})),
                "publicationAt": nullable(timestamp()),
                "cover": nullable(r("MediaUseReference")),
                "featured": {"type": "boolean"}
            }),
        ),
    );
    s.insert(
        "FaqTypeFields".into(),
        object(
            &["items"],
            json!({"items": {"type": "array", "items": r("FaqItem")}}),
        ),
    );
    s.insert(
        "CaseStudyTypeFields".into(),
        object(
            &[],
            json!({
                "industry": nullable(json!({"type": "string", "maxLength": 200})),
                "location": nullable(json!({"type": "string", "maxLength": 200}))
            }),
        ),
    );
    s.insert(
        "DownloadTypeFields".into(),
        object(
            &[],
            json!({
                "versionLabel": nullable(json!({"type": "string", "maxLength": 200})),
                "resourceType": nullable(json!({"type": "string", "maxLength": 120})),
                "versionNotes": nullable(json!({"type": "string", "maxLength": 2000}))
            }),
        ),
    );
    s.insert(
        "LegalTypeFields".into(),
        object(
            &[],
            json!({"effectiveDate": nullable(json!({"type": "string"}))}),
        ),
    );
}

fn add_general_information(s: &mut Map<String, Value>) {
    s.insert(
        "ContactInformationInput".into(),
        object(
            &["addressLines"],
            json!({
                "email": nullable(json!({"type": "string", "format": "email", "maxLength": 254})),
                "phone": nullable(json!({"type": "string", "maxLength": 50})),
                "addressLines": {"type": "array", "items": {"type": "string", "maxLength": 300}},
                "locality": nullable(json!({"type": "string", "maxLength": 200})),
                "region": nullable(json!({"type": "string", "maxLength": 200})),
                "postalCode": nullable(json!({"type": "string", "maxLength": 50})),
                "countryCode": nullable(json!({"type": "string", "maxLength": 2}))
            }),
        ),
    );
    s.insert(
        "SocialLinkInput".into(),
        object(
            &["service", "url"],
            json!({
                "service": {"type": "string", "maxLength": 120},
                "url": {"type": "string", "format": "uri", "maxLength": 2048, "pattern": "^https://"}
            }),
        ),
    );
    s.insert(
        "ProductCategoryPresentationInput".into(),
        object(
            &["code", "slug", "name", "description", "sortOrder"],
            json!({
                "code": r("ProductFamily"), "slug": editable_slug(),
                "name": {"type": "string", "maxLength": 120},
                "description": {"type": "string", "maxLength": 2000},
                "sortOrder": {"type": "integer", "format": "int32"}
            }),
        ),
    );
    s.insert(
        "GeneralInformationTypeFields".into(),
        object(
            &["contact", "socialLinks", "defaultSeo", "productCategories"],
            json!({
                "organizationName": nullable(json!({"type": "string", "maxLength": 200})),
                "brandLine": nullable(json!({"type": "string", "maxLength": 500})),
                "siteIcon": nullable(r("AssetVersionReference")),
                "homePath": nullable(json!({
                    "type": "string", "maxLength": 2048,
                    "pattern": "^/(?![/\\\\])[^?#\\s\\\\]*$"
                })),
                "footerStatement": nullable(json!({"type": "string", "maxLength": 1000})),
                "copyrightTemplate": nullable(json!({"type": "string", "maxLength": 500})),
                "contact": r("ContactInformationInput"),
                "socialLinks": {"type": "array", "items": r("SocialLinkInput")},
                "defaultSeo": r("SeoInputV2"),
                "productCategories": {
                    "type": "array", "items": r("ProductCategoryPresentationInput")
                },
                "navigationCta": nullable(r("EditorialAction"))
            }),
        ),
    );
}

fn add_navigation_and_footer(s: &mut Map<String, Value>) {
    s.insert(
        "NavigationItem".into(),
        object(
            &["id", "label", "children"],
            json!({
                "id": uuid(),
                "label": {"type": "string", "maxLength": 120},
                "target": nullable(r("LinkTargetReference")),
                "children": {"type": "array", "items": r("NavigationItem")}
            }),
        ),
    );
    s.insert(
        "FooterColumn".into(),
        object(
            &["id", "title", "links"],
            json!({
                "id": uuid(),
                "title": {"type": "string", "maxLength": 120},
                "links": {"type": "array", "items": r("NavigationItem")}
            }),
        ),
    );
    s.insert(
        "NavigationTypeFields".into(),
        object(
            &["items"],
            json!({"items": {"type": "array", "items": r("NavigationItem")}}),
        ),
    );
    s.insert(
        "FooterTypeFields".into(),
        object(
            &["columns", "legalLinks"],
            json!({
                "columns": {"type": "array", "items": r("FooterColumn")},
                "legalLinks": {"type": "array", "items": r("NavigationItem")}
            }),
        ),
    );
}

fn add_type_fields_union(s: &mut Map<String, Value>) {
    let variants = vec![
        typed_object("type", "home", &[], json!({})),
        typed_object("type", "page", &[], json!({})),
        tagged_payload("solution", &s["TaxonomyTypeFields"]),
        tagged_payload("technology", &s["TaxonomyTypeFields"]),
        tagged_payload("article", &s["EditorialTypeFields"]),
        tagged_payload("news", &s["EditorialTypeFields"]),
        tagged_payload("faq", &s["FaqTypeFields"]),
        tagged_payload("caseStudy", &s["CaseStudyTypeFields"]),
        tagged_payload("download", &s["DownloadTypeFields"]),
        typed_object("type", "company", &[], json!({})),
        tagged_payload("legal", &s["LegalTypeFields"]),
        tagged_payload("generalInformation", &s["GeneralInformationTypeFields"]),
        tagged_payload("navigation", &s["NavigationTypeFields"]),
        tagged_payload("footer", &s["FooterTypeFields"]),
    ];
    s.insert(
        "ContentTypeFields".into(),
        json!({"oneOf": variants, "discriminator": {"propertyName": "type"}}),
    );
}

fn tagged_payload(kind: &str, payload: &Value) -> Value {
    let mut properties = payload["properties"]
        .as_object()
        .cloned()
        .expect("payload properties");
    properties.insert("type".into(), json!({"type": "string", "const": kind}));
    let mut required = payload["required"].as_array().cloned().expect("required");
    required.insert(0, json!("type"));
    json!({
        "type": "object", "additionalProperties": false,
        "required": required, "properties": properties
    })
}

fn add_draft(s: &mut Map<String, Value>) {
    s.insert(
        "ContentDraftV2".into(),
        object(
            &[
                "schemaVersion",
                "kind",
                "locale",
                "templateKey",
                "title",
                "isPlaceholder",
                "typeFields",
                "composition",
                "seo",
                "relations",
                "draftVersion",
            ],
            json!({
                "schemaVersion": {"type": "integer", "const": 2},
                "kind": {"allOf": [r("CmsContentKind")], "x-immutable": true},
                "locale": {
                    "type": "string", "minLength": 2, "maxLength": 35,
                    "pattern": "^[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*$", "x-immutable": true
                },
                "templateKey": {"allOf": [r("ContentTemplateKey")], "x-immutable": true},
                "title": {"type": "string", "maxLength": 300},
                "slug": nullable(editable_slug()),
                "summary": nullable(json!({"type": "string", "maxLength": 2000})),
                "isPlaceholder": {"type": "boolean"},
                "typeFields": r("ContentTypeFields"),
                "body": nullable(r("TiptapDocument")),
                "composition": r("PageComposition"),
                "seo": r("SeoInputV2"),
                "relations": {"type": "array", "items": r("ContentRelationReference")},
                "draftVersion": {"type": "integer", "format": "int64"}
            }),
        ),
    );
}

fn editable_slug() -> Value {
    json!({
        "type": "string",
        "maxLength": 180,
        "pattern": "^(?:[a-z0-9]+(?:-[a-z0-9]+)*)?$"
    })
}
