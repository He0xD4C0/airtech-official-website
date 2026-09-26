//! Canonical public-site search schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "PublicSearchEntityType".into(),
        string_enum(&["content", "product"]),
    );
    s.insert(
        "PublicSearchType".into(),
        string_enum(&[
            "product",
            "solution",
            "technology",
            "article",
            "news",
            "faq",
            "caseStudy",
            "download",
            "company",
            "page",
        ]),
    );
    s.insert(
        "PublicSearchItem".into(),
        object(
            &[
                "entityType",
                "entityId",
                "title",
                "summary",
                "canonicalPath",
                "displayType",
            ],
            json!({
                "entityType": r("PublicSearchEntityType"),
                "entityId": uuid(),
                "title": {"type": "string", "minLength": 1},
                "summary": nullable(json!({"type": "string"})),
                "canonicalPath": {"type": "string", "pattern": "^/en(?:/|$)"},
                "displayType": r("PublicSearchType")
            }),
        ),
    );
    s.insert(
        "PublicSearchPage".into(),
        object(
            &["items", "nextCursor", "total", "typeCounts"],
            json!({
                "items": array(r("PublicSearchItem")),
                "nextCursor": nullable(json!({"type": "string", "minLength": 1, "maxLength": 2048, "pattern": "^[A-Za-z0-9_-]+$"})),
                "total": {"type": "integer", "minimum": 0},
                "typeCounts": array(r("ProductFacetCount"))
            }),
        ),
    );
}
