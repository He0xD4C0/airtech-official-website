//! Published product and selector schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "FactState".into(),
        string_enum(&[
            "verified",
            "missing",
            "notApplicable",
            "notTested",
            "confidential",
            "pendingVerification",
        ]),
    );
    s.insert(
        "ProductFamily".into(),
        string_enum(&["centrifugal", "axial", "crossFlow", "inlineDuct", "motors"]),
    );
    s.insert("SpecValue".into(), object(
        &["key", "label", "value", "unit", "operatingCondition", "state", "sourceReference"],
        json!({"key": {"type": "string"}, "label": {"type": "string"}, "value": nullable(json!({})), "unit": nullable(json!({"type": "string"})), "operatingCondition": nullable(json!({"type": "string"})), "state": r("FactState"), "sourceReference": nullable(json!({"type": "string"}))})
    ));
    s.insert(
        "PerformancePoint".into(),
        object(
            &["airflow", "pressure"],
            json!({"airflow": {"type": "number"}, "pressure": {"type": "number"}}),
        ),
    );
    s.insert("PerformanceCurve".into(), object(
        &["airflowUnit", "pressureUnit", "speedRpm", "densityKgM3", "voltage", "testMethod", "sourceReference", "state", "points"],
        json!({
            "airflowUnit": {"type": "string"}, "pressureUnit": {"type": "string"}, "speedRpm": nullable(json!({"type": "integer", "minimum": 0})),
            "densityKgM3": nullable(json!({"type": "number"})), "voltage": nullable(json!({"type": "string"})), "testMethod": nullable(json!({"type": "string"})),
            "sourceReference": {"type": "string", "minLength": 1}, "state": r("FactState"), "points": array(r("PerformancePoint"))
        })
    ));
    s.insert("Product".into(), object(
        &["id", "stableId", "model", "slug", "locale", "family", "subtype", "motorTechnology", "title", "summary", "seo", "sortOrder", "relatedContentIds", "specifications", "performanceCurves", "sourceSnapshotId", "sourceRevision", "currentRevision", "publishedRevision", "status", "indexable", "updatedAt"],
        json!({
            "id": uuid(), "stableId": {"type": "string"}, "model": nullable(json!({"type": "string"})), "slug": slug(), "locale": {"type": "string"},
            "family": r("ProductFamily"), "subtype": nullable(json!({"type": "string"})), "motorTechnology": nullable(json!({"type": "string"})),
            "title": {"type": "string"}, "summary": nullable(json!({"type": "string"})), "seo": r("SeoMetadata"),
            "sortOrder": {"type": "integer"}, "relatedContentIds": array(uuid()),
            "specifications": array(r("SpecValue")), "performanceCurves": array(r("PerformanceCurve")),
            "sourceSnapshotId": uuid(), "sourceRevision": {"type": "string"}, "currentRevision": revision(), "publishedRevision": nullable(revision()),
            "status": r("PublicationStatus"), "indexable": {"type": "boolean"}, "updatedAt": timestamp()
        })
    ));
    s.insert("ProductPage".into(), product_page());
    s.insert(
        "ProductFacetCount".into(),
        object(
            &["value", "count"],
            json!({"value": {"type": "string"}, "count": {"type": "integer", "minimum": 0}}),
        ),
    );
    s.insert(
        "AdminProductPage".into(),
        object(
            &[
                "items",
                "nextCursor",
                "total",
                "familyCounts",
                "statusCounts",
                "dataStateCounts",
            ],
            json!({
                "items": array(r("Product")),
                "nextCursor": nullable(json!({"type": "string"})),
                "total": {"type": "integer", "minimum": 0},
                "familyCounts": array(r("ProductFacetCount")),
                "statusCounts": array(r("ProductFacetCount")),
                "dataStateCounts": array(r("ProductFacetCount"))
            }),
        ),
    );
    s.insert("ProductPublicationAction".into(), string_enum(&["publish"]));
    s.insert(
        "ValidationIssue".into(),
        object(
            &["fieldPath", "code", "detail"],
            json!({
                "fieldPath": {"type": "string"},
                "code": {"type": "string"},
                "detail": {"type": "string"}
            }),
        ),
    );
    s.insert(
        "ProductPublicationReport".into(),
        object(
            &[
                "productId",
                "currentRevision",
                "ready",
                "issues",
                "allowedActions",
            ],
            json!({
                "productId": uuid(),
                "currentRevision": revision(),
                "ready": {"type": "boolean"},
                "issues": array(r("ValidationIssue")),
                "allowedActions": array(r("ProductPublicationAction"))
            }),
        ),
    );
    s.insert(
        "SelectorPriority".into(),
        string_enum(&["efficiency", "noise", "size", "headroom"]),
    );
    s.insert("SelectorRequest".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["airflow", "airflowUnit", "pressure", "pressureUnit", "requiredCertifications"],
        "properties": {
            "airflow": {"type": "number", "exclusiveMinimum": 0}, "airflowUnit": {"type": "string"}, "pressure": {"type": "number", "exclusiveMinimum": 0}, "pressureUnit": {"type": "string"},
            "ambientTemperatureC": nullable(json!({"type": "number"})), "maximumDiameterMm": nullable(json!({"type": "number", "exclusiveMinimum": 0})),
            "voltage": nullable(json!({"type": "string"})), "frequencyHz": nullable(json!({"type": "number", "exclusiveMinimum": 0})),
            "motorTechnology": nullable(json!({"type": "string", "minLength": 1, "maxLength": 120})),
            "requiredCertifications": array(json!({"type": "string"})), "preferredFamily": nullable(r("ProductFamily")), "priority": nullable(r("SelectorPriority"))
        }
    }));
    s.insert("SelectorCandidate".into(), object(
        &["productId", "productRevision", "title", "matchedConstraints", "warnings", "rank"],
        json!({"productId": uuid(), "productRevision": revision(), "title": {"type": "string"}, "matchedConstraints": array(json!({"type": "string"})), "warnings": array(json!({"type": "string"})), "rank": {"type": "integer", "minimum": 0}})
    ));
    s.insert(
        "SelectorOutcome".into(),
        string_enum(&[
            "matched",
            "noValidatedCandidates",
            "engineeringReviewRequired",
        ]),
    );
    s.insert("SelectorResponse".into(), object(&["outcome", "candidates", "explanations"], json!({"outcome": r("SelectorOutcome"), "candidates": array(r("SelectorCandidate")), "explanations": array(json!({"type": "string"}))})));
}
