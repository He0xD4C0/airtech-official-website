//! Editorial, site-bootstrap, and route-resolution schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

/// Adds database-backed editorial and public-site schemas.
pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "DataClass".into(),
        string_enum(&["editorial", "feishu", "verifiedCsv", "developmentFixture"]),
    );
    s.insert(
        "NewsDraftInput".into(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["content", "category", "authorDisplayName"],
            "properties": {
                "content": r("ContentDraftInput"),
                "category": {"type": "string", "minLength": 1, "maxLength": 120},
                "authorDisplayName": {"type": "string", "minLength": 1, "maxLength": 200},
                "coverMediaId": nullable(uuid()),
                "publishedAt": nullable(timestamp()),
                "featured": {"type": "boolean", "default": false},
                "dataClass": {"allOf": [r("DataClass")], "default": "editorial"}
            }
        }),
    );
    s.insert(
        "NewsEntry".into(),
        object(
            &[
                "content",
                "category",
                "authorDisplayName",
                "coverMediaId",
                "publishedAt",
                "featured",
                "dataClass",
            ],
            json!({
                "content": r("PublicContentProjection"),
                "category": {"type": "string"},
                "authorDisplayName": nullable(json!({"type": "string"})),
                "coverMediaId": nullable(uuid()),
                "publishedAt": nullable(timestamp()),
                "featured": {"type": "boolean"},
                "dataClass": r("DataClass")
            }),
        ),
    );
    s.insert(
        "NewsRevision".into(),
        json!({
            "description": "Immutable News revision payload.",
            "allOf": [r("NewsEntry")]
        }),
    );
    s.insert("NewsRevisionPage".into(), page("NewsRevision"));
    s.insert("NewsPage".into(), page("NewsEntry"));

    s.insert(
        "GeneralInformationPayload".into(),
        json!({
            "type": "object",
            "additionalProperties": true,
            "required": [
                "brandName",
                "brandLine",
                "homePath",
                "footerStatement",
                "copyrightText",
                "defaultSeo",
                "organization"
            ],
            "properties": {
                "brandName": {"type": "string"},
                "brandLine": nullable(json!({"type": "string"})),
                "homePath": {"type": "string", "pattern": "^/en(?:/|$)"},
                "footerStatement": nullable(json!({"type": "string"})),
                "copyrightText": nullable(json!({"type": "string"})),
                "defaultSeo": {"type": "object", "additionalProperties": true},
                "organization": {"type": "object", "additionalProperties": true},
                "navigationCta": nullable(json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "label": {"type": "string"},
                        "href": {"type": "string", "pattern": "^/en(?:/|$)"}
                    }
                })),
                "productCategories": {"type": "array", "items": r("ProductFamilyPresentation")}
            }
        }),
    );
    s.insert(
        "GeneralInformationDraftInput".into(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["payload"],
            "properties": {
                "locale": {"type": "string", "default": "en"},
                "payload": r("GeneralInformationPayload"),
                "isPlaceholder": {"type": "boolean", "default": false}
            }
        }),
    );
    s.insert(
        "GeneralInformation".into(),
        object(
            &[
                "id",
                "locale",
                "payload",
                "status",
                "currentRevision",
                "publishedRevision",
                "isPlaceholder",
                "updatedAt",
            ],
            json!({
                "id": uuid(),
                "locale": {"type": "string"},
                "payload": r("GeneralInformationPayload"),
                "status": r("PublicationStatus"),
                "currentRevision": revision(),
                "publishedRevision": nullable(revision()),
                "isPlaceholder": {"type": "boolean"},
                "updatedAt": timestamp()
            }),
        ),
    );
    s.insert(
        "GeneralInformationRevision".into(),
        json!({
            "description": "Immutable General Information revision payload.",
            "allOf": [r("GeneralInformation")]
        }),
    );
    s.insert(
        "GeneralInformationRevisionPage".into(),
        page("GeneralInformationRevision"),
    );
    s.insert(
        "ProductFamilyPresentation".into(),
        object(
            &["code", "slug", "name", "description", "sortOrder"],
            json!({
                "code": r("ProductFamily"),
                "slug": slug(),
                "name": {"type": "string"},
                "description": {"type": "string"},
                "sortOrder": {"type": "integer"}
            }),
        ),
    );
    add_public_projection_schemas(s);
    s.insert(
        "SiteBootstrap".into(),
        object(
            &[
                "generalInformation",
                "navigation",
                "footer",
                "productFamilies",
                "motorTechnologies",
                "generatedAt",
            ],
            json!({
                "generalInformation": nullable(r("PublicContentProjection")),
                "navigation": nullable(r("PublicContentProjection")),
                "footer": nullable(r("PublicContentProjection")),
                "productFamilies": array(r("ProductFamilyPresentation")),
                "motorTechnologies": array(json!({"type": "string"})),
                "generatedAt": timestamp()
            }),
        ),
    );
    s.insert(
        "RouteResolution".into(),
        object(
            &[
                "path",
                "templateKey",
                "entityType",
                "entityId",
                "locale",
                "publishedRevision",
                "indexable",
                "dataClass",
                "page",
            ],
            json!({
                "path": {"type": "string"},
                "templateKey": {"type": "string"},
                "entityType": {"type": "string"},
                "entityId": nullable(uuid()),
                "locale": {"type": "string"},
                "publishedRevision": nullable(revision()),
                "indexable": {"type": "boolean"},
                "dataClass": r("DataClass"),
                "page": nullable(r("PublicContentProjection"))
            }),
        ),
    );
}

fn add_public_projection_schemas(s: &mut Map<String, Value>) {
    s.insert(
        "ResolvedRelationCard".into(),
        object(
            &[
                "relationId",
                "entityType",
                "title",
                "summary",
                "href",
                "eyebrow",
                "tags",
            ],
            json!({
                "relationId": uuid(),
                "entityType": string_enum(&["content", "product"]),
                "title": {"type": "string"},
                "summary": nullable(json!({"type": "string"})),
                "href": {"type": "string"},
                "eyebrow": nullable(json!({"type": "string"})),
                "tags": array(json!({"type": "string"}))
            }),
        ),
    );
    s.insert(
        "ResolvedLinkTarget".into(),
        object(
            &["contentId", "href"],
            json!({
                "contentId": uuid(),
                "href": {"type": "string"}
            }),
        ),
    );
    s.insert(
        "PublicContentProjection".into(),
        object(
            &[
                "schemaVersion",
                "id",
                "kind",
                "locale",
                "templateKey",
                "title",
                "slug",
                "summary",
                "isPlaceholder",
                "typeFields",
                "body",
                "composition",
                "seo",
                "publishedRevision",
                "updatedAt",
                "resolvedRelations",
                "resolvedLinks",
                "resolvedMedia",
            ],
            json!({
                "schemaVersion": {"type": "integer", "const": 2},
                "id": uuid(),
                "kind": r("CmsContentKind"),
                "locale": {"type": "string"},
                "templateKey": r("ContentTemplateKey"),
                "title": {"type": "string"},
                "slug": nullable(json!({"type": "string"})),
                "summary": nullable(json!({"type": "string"})),
                "isPlaceholder": {"type": "boolean"},
                "typeFields": r("ContentTypeFields"),
                "body": nullable(r("TiptapDocument")),
                "composition": r("PageComposition"),
                "seo": r("SeoInputV2"),
                "publishedRevision": revision(),
                "updatedAt": timestamp(),
                "resolvedRelations": array(r("ResolvedRelationCard")),
                "resolvedLinks": array(r("ResolvedLinkTarget")),
                "resolvedMedia": array(r("ResolvedMedia"))
            }),
        ),
    );
}
