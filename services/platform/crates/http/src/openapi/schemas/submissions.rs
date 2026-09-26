//! RFQ, contact, consent, analytics-event, and discovery schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "RfqJourney".into(),
        string_enum(&["product", "selection", "project", "replacement"]),
    );
    s.insert("BusinessContact".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["name", "email"],
        "properties": {
            "name": {"type": "string", "minLength": 1, "maxLength": 200}, "email": {"type": "string", "format": "email", "maxLength": 320},
            "phone": nullable(json!({"type": "string", "minLength": 1, "maxLength": 50})), "company": nullable(json!({"type": "string", "minLength": 1, "maxLength": 200})),
            "countryOrRegion": nullable(json!({"type": "string", "minLength": 1, "maxLength": 120}))
        }
    }));
    s.insert("ProductContext".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["productId", "stableId", "publishedRevision"],
        "properties": {"productId": uuid(), "stableId": {"type": "string", "minLength": 1, "maxLength": 200}, "model": nullable(json!({"type": "string", "minLength": 1, "maxLength": 200})), "publishedRevision": revision()}
    }));
    s.insert("ProductRfqProductContext".into(), object(
        &["productId", "stableId", "model", "publishedRevision"],
        json!({
            "productId": uuid(), "stableId": {"type": "string", "minLength": 1, "maxLength": 200},
            "model": {"type": "string", "minLength": 1, "maxLength": 200}, "publishedRevision": revision()
        })
    ));
    s.insert(
        "RfqQuantity".into(),
        json!({
            "oneOf": [
                {"type": "integer", "minimum": 1, "maximum": 1000000},
                {"type": "string", "pattern": "^[0-9]{1,7}$"}
            ]
        }),
    );
    s.insert(
        "RfqDutyPoint".into(),
        object(
            &["airflow", "airflowUnit", "pressure", "pressureUnit"],
            json!({
                "airflow": {"type": "number", "exclusiveMinimum": 0, "maximum": 1000000000},
                "airflowUnit": string_enum(&["m3/h", "m³/h", "CFM", "cfm"]),
                "pressure": {"type": "number", "exclusiveMinimum": 0, "maximum": 100000000},
                "pressureUnit": string_enum(&["Pa", "pa", "kPa", "kpa", "inH2O", "inh2o"])
            }),
        ),
    );
    s.insert(
        "RfqElectricalContext".into(),
        json!({
            "type": "object", "additionalProperties": false, "minProperties": 1,
            "properties": {
                "voltage": {"type": "string", "minLength": 1, "maxLength": 40},
                "frequencyHz": {"type": "number", "exclusiveMinimum": 0, "maximum": 1000}
            }
        }),
    );
    s.insert(
        "ProductRfqContext".into(),
        object(&["application"], Value::Object(rfq_context_properties([]))),
    );
    s.insert(
        "SelectionRfqContext".into(),
        object(
            &["application", "dutyPoint"],
            Value::Object(rfq_context_properties([
                ("dutyPoint", r("RfqDutyPoint")),
                ("ambientTemperatureC", json!({"type": "number", "minimum": -273.15, "maximum": 1000})),
                ("preferredFamily", r("ProductFamily")),
                ("motorTechnology", json!({"type": "string", "minLength": 1, "maxLength": 120})),
                ("maximumDiameterMm", json!({"type": "number", "exclusiveMinimum": 0, "maximum": 100000})),
                ("requiredCertifications", json!({"type": "array", "maxItems": 20, "items": {"type": "string", "minLength": 1, "maxLength": 120}})),
                ("control", json!({"type": "string", "minLength": 1, "maxLength": 120})),
            ])),
        ),
    );
    s.insert(
        "ProjectRfqContext".into(),
        object(
            &["application", "projectStage"],
            Value::Object(rfq_context_properties([
                (
                    "projectStage",
                    string_enum(&["Concept", "Engineering", "Prototype", "Production planning"]),
                ),
                (
                    "projectScale",
                    json!({"type": "string", "minLength": 1, "maxLength": 500}),
                ),
                (
                    "schedule",
                    json!({"type": "string", "minLength": 1, "maxLength": 500}),
                ),
                (
                    "engineeringNeeds",
                    json!({"type": "string", "minLength": 1, "maxLength": 4000}),
                ),
            ])),
        ),
    );
    s.insert(
        "ReplacementRfqContext".into(),
        object(
            &["application", "existingModel", "dutyPoint"],
            Value::Object(rfq_context_properties([
                (
                    "existingModel",
                    json!({"type": "string", "minLength": 1, "maxLength": 300}),
                ),
                ("dutyPoint", r("RfqDutyPoint")),
                (
                    "installationConstraints",
                    json!({"type": "string", "minLength": 1, "maxLength": 4000}),
                ),
                (
                    "replacementGoal",
                    json!({"type": "string", "minLength": 1, "maxLength": 2000}),
                ),
            ])),
        ),
    );
    s.insert(
        "ProductRfqRequest".into(),
        rfq_request_schema("product", "ProductRfqContext", true),
    );
    s.insert(
        "SelectionRfqRequest".into(),
        rfq_request_schema("selection", "SelectionRfqContext", false),
    );
    s.insert(
        "ProjectRfqRequest".into(),
        rfq_request_schema("project", "ProjectRfqContext", false),
    );
    s.insert(
        "ReplacementRfqRequest".into(),
        rfq_request_schema("replacement", "ReplacementRfqContext", false),
    );
    s.insert("CreateRfqRequest".into(), json!({
        "oneOf": [r("ProductRfqRequest"), r("SelectionRfqRequest"), r("ProjectRfqRequest"), r("ReplacementRfqRequest")],
        "discriminator": {
            "propertyName": "journey",
            "mapping": {
                "product": "#/components/schemas/ProductRfqRequest",
                "selection": "#/components/schemas/SelectionRfqRequest",
                "project": "#/components/schemas/ProjectRfqRequest",
                "replacement": "#/components/schemas/ReplacementRfqRequest"
            }
        }
    }));
    s.insert("CreateContactRequest".into(), object(
        &["contact", "topic", "message", "sourcePath", "locale", "consent"],
        json!({"contact": r("BusinessContact"), "topic": {"type": "string"}, "message": {"type": "string"}, "sourcePath": {"type": "string"}, "locale": {"type": "string"}, "consent": {"type": "boolean"}})
    ));
    s.insert(
        "AcceptedResponse".into(),
        object(
            &["id", "reference", "acceptedAt"],
            json!({"id": uuid(), "reference": {"type": "string"}, "acceptedAt": timestamp()}),
        ),
    );
    add_business_inbox_schemas(s);
    s.insert(
        "AnalyticsPolicyVersion".into(),
        string_enum(&["analytics-v1"]),
    );
    s.insert(
        "CreateAnalyticsConsent".into(),
        object(
            &["anonymousSessionId", "policyVersion", "analyticsAllowed"],
            json!({
                "anonymousSessionId": uuid(), "policyVersion": r("AnalyticsPolicyVersion"),
                "analyticsAllowed": {"type": "boolean"}
            }),
        ),
    );
    s.insert("AnalyticsConsentReceipt".into(), object(
        &["consentReceipt", "anonymousSessionId", "policyVersion", "analyticsAllowed", "grantedAt", "expiresAt"],
        json!({
            "consentReceipt": uuid(), "anonymousSessionId": uuid(), "policyVersion": r("AnalyticsPolicyVersion"),
            "analyticsAllowed": {"type": "boolean"}, "grantedAt": timestamp(), "expiresAt": timestamp()
        })
    ));
    s.insert(
        "AnalyticsEventName".into(),
        string_enum(&[
            "pageView",
            "internalSearch",
            "filterApplied",
            "selectorStarted",
            "selectorStepCompleted",
            "selectorResult",
            "compareChanged",
            "downloadStarted",
            "faqExpanded",
            "ctaClicked",
            "rfqRouteSelected",
            "rfqStarted",
            "rfqStepCompleted",
            "rfqValidationError",
            "rfqSubmitted",
            "rfqSubmitFailed",
        ]),
    );
    s.insert(
        "AnalyticsEventProperties".into(),
        analytics_properties_schema(),
    );
    s.insert("ConsentedAnalyticsEvent".into(), object(
        &["eventName", "anonymousSessionId", "sourcePath", "locale", "consentGranted", "policyVersion", "consentReceipt"],
        json!({
            "eventName": r("AnalyticsEventName"), "anonymousSessionId": uuid(),
            "sourcePath": {"type": "string", "minLength": 3, "maxLength": 2048, "pattern": "^/en(?:/|$)[^?#]*$"},
            "locale": string_enum(&["en"]), "consentGranted": {"type": "boolean", "enum": [true]},
            "policyVersion": r("AnalyticsPolicyVersion"), "consentReceipt": uuid(),
            "properties": r("AnalyticsEventProperties")
        })
    ));
    s.insert("AnalyticsOptOutEvent".into(), object(
        &["eventName", "sourcePath", "locale", "consentGranted"],
        json!({
            "eventName": r("AnalyticsEventName"),
            "sourcePath": {"type": "string", "minLength": 3, "maxLength": 2048, "pattern": "^/en(?:/|$)[^?#]*$"},
            "locale": string_enum(&["en"]), "consentGranted": {"type": "boolean", "enum": [false]},
            "properties": {"type": "object", "maxProperties": 0, "additionalProperties": false, "default": {}}
        })
    ));
    s.insert(
        "CreateAnalyticsEvent".into(),
        json!({
            "oneOf": [r("ConsentedAnalyticsEvent"), r("AnalyticsOptOutEvent")]
        }),
    );
    s.insert(
        "AnalyticsEventReceipt".into(),
        object(
            &["accepted", "eventId"],
            json!({"accepted": {"type": "boolean"}, "eventId": nullable(uuid())}),
        ),
    );
    s.insert("DiscoveryEntry".into(), object(
        &["entityType", "entityId", "path", "locale", "title", "summary", "updatedAt"],
        json!({
            "entityType": string_enum(&["content", "product"]), "entityId": uuid(), "path": {"type": "string", "pattern": "^/en(?:/|$)"},
            "displayType": r("PublicSearchType"),
            "locale": {"type": "string"}, "title": {"type": "string"}, "summary": nullable(json!({"type": "string"})), "updatedAt": timestamp()
        })
    ));
    s.insert(
        "DiscoveryDocument".into(),
        object(
            &["generatedAt", "entries"],
            json!({"generatedAt": timestamp(), "entries": array(r("DiscoveryEntry"))}),
        ),
    );
}

fn add_business_inbox_schemas(s: &mut Map<String, Value>) {
    s.insert(
        "BusinessEntityType".into(),
        string_enum(&["rfq", "contact"]),
    );
    s.insert(
        "BusinessInboxStatus".into(),
        string_enum(&[
            "new",
            "triaged",
            "assigned",
            "qualified",
            "closed",
            "spam",
            "piiCleared",
        ]),
    );
    s.insert("BusinessInboxItem".into(), object(
        &["id", "entityType", "reference", "journey", "topic", "organization", "countryOrRegion", "productContext", "sourcePath", "locale", "consent", "status", "revision", "assignedTo", "submittedAt", "updatedAt", "retentionUntil"],
        json!({
            "id": uuid(), "entityType": r("BusinessEntityType"), "reference": {"type": "string"},
            "journey": nullable(r("RfqJourney")), "topic": nullable(json!({"type": "string"})),
            "organization": nullable(json!({"type": "string"})), "countryOrRegion": nullable(json!({"type": "string"})),
            "productContext": nullable(r("ProductContext")), "sourcePath": {"type": "string"}, "locale": {"type": "string"},
            "consent": {"type": "boolean"}, "status": r("BusinessInboxStatus"), "revision": revision(),
            "assignedTo": nullable(uuid()), "submittedAt": timestamp(), "updatedAt": timestamp(), "retentionUntil": timestamp()
        })
    ));
    s.insert(
        "BusinessInboxPage".into(),
        object(
            &["items", "nextCursor", "total"],
            json!({
                "items": array(r("BusinessInboxItem")),
                "nextCursor": nullable(json!({"type": "string"})),
                "total": {"type": "integer", "minimum": 0}
            }),
        ),
    );
    s.insert("RfqContextSnapshot".into(), json!({"oneOf": [
        object(&["journey","context"],json!({"journey": {"const":"product","type":"string"}, "context":r("ProductRfqContext")})),
        object(&["journey","context"],json!({"journey": {"const":"selection","type":"string"}, "context":r("SelectionRfqContext")})),
        object(&["journey","context"],json!({"journey": {"const":"project","type":"string"}, "context":r("ProjectRfqContext")})),
        object(&["journey","context"],json!({"journey": {"const":"replacement","type":"string"}, "context":r("ReplacementRfqContext")}))
    ]}));
    s.insert("BusinessPii".into(), object(
        &["name", "email", "phone", "company", "countryOrRegion", "message"],
        json!({
            "rfqContext": nullable(r("RfqContextSnapshot")), "name": {"type": "string"}, "email": {"type": "string", "format": "email"},
            "phone": nullable(json!({"type": "string"})), "company": nullable(json!({"type": "string"})),
            "countryOrRegion": nullable(json!({"type": "string"})), "message": nullable(json!({"type": "string"}))
        })
    ));
    s.insert("BusinessInternalNote".into(), object(
        &["id", "entityType", "entityId", "body", "createdBy", "createdAt"],
        json!({"id": uuid(), "entityType": r("BusinessEntityType"), "entityId": uuid(), "body": {"type": "string"}, "createdBy": uuid(), "createdAt": timestamp()})
    ));
    s.insert("BusinessStatusHistoryEntry".into(), object(
        &["id", "fromStatus", "toStatus", "reason", "changedBy", "changedAt"],
        json!({"id": uuid(), "fromStatus": nullable(r("BusinessInboxStatus")), "toStatus": r("BusinessInboxStatus"), "reason": nullable(json!({"type": "string"})), "changedBy": {"type": "string"}, "changedAt": timestamp()})
    ));
    s.insert("BusinessInboxDetail".into(), object(
        &["item", "notes", "statusHistory"],
        json!({"item": r("BusinessInboxItem"), "notes": array(r("BusinessInternalNote")), "statusHistory": array(r("BusinessStatusHistoryEntry"))})
    ));
    s.insert("AssignBusinessInboxRequest".into(), object(
        &["assignedTo", "reason"],
        json!({"assignedTo": nullable(uuid()), "reason": {"type": "string", "minLength": 3, "maxLength": 2000}})
    ));
    s.insert("UpdateBusinessStatusRequest".into(), object(
        &["status", "reason"],
        json!({"status": r("BusinessInboxStatus"), "reason": {"type": "string", "minLength": 3, "maxLength": 2000}})
    ));
    s.insert("CreateBusinessNoteRequest".into(), object(
        &["body", "reason"],
        json!({"body": {"type": "string", "minLength": 1, "maxLength": 4000}, "reason": {"type": "string", "minLength": 3, "maxLength": 2000}})
    ));
}

fn rfq_context_properties<const N: usize>(extra: [(&str, Value); N]) -> Map<String, Value> {
    let mut properties = json!({
        "application": {"type": "string", "minLength": 1, "maxLength": 500},
        "quantity": r("RfqQuantity"),
        "electrical": r("RfqElectricalContext"),
        "environment": {"type": "string", "minLength": 1, "maxLength": 4000},
        "priority": string_enum(&["efficiency", "noise", "size", "headroom"]),
        "additionalMessage": {"type": "string", "minLength": 1, "maxLength": 10000}
    })
    .as_object()
    .expect("RFQ properties object")
    .clone();
    properties.extend(
        extra
            .into_iter()
            .map(|(name, schema)| (name.to_owned(), schema)),
    );
    properties
}

fn rfq_request_schema(journey: &str, context: &str, product_context: bool) -> Value {
    let mut required = vec![
        "journey",
        "contact",
        "sourcePath",
        "locale",
        "consent",
        "context",
    ];
    let mut properties = json!({
        "journey": {"type": "string", "enum": [journey]},
        "contact": r("BusinessContact"),
        "sourcePath": {"type": "string", "minLength": 3, "maxLength": 2048, "pattern": "^/en/request-a-quote(?:/|$)[^?#]*$"},
        "locale": string_enum(&["en"]),
        "consent": {"type": "boolean", "enum": [true]},
        "context": r(context)
    })
    .as_object()
    .expect("RFQ request properties")
    .clone();
    if product_context {
        required.push("productContext");
        properties.insert("productContext".into(), r("ProductRfqProductContext"));
    }
    object(&required, Value::Object(properties))
}

fn analytics_properties_schema() -> Value {
    let bounded_count = || json!({"type": "integer", "minimum": 0, "maximum": 1000000});
    let faq_id = || {
        json!({
            "oneOf": [
                uuid(),
                {"type": "string", "pattern": "^faq-[1-9][0-9]{0,5}$"}
            ]
        })
    };
    json!({
        "description": "A scalar-only property object. Every string is a UUID, a numeric FAQ identifier, or a server-controlled enum; the server also selects the matching property keys from eventName.",
        "anyOf": [
            object(&[], json!({"contentKind": string_enum(&["content", "news", "product"]), "contentId": uuid(), "publishedRevision": revision()})),
            object(&[], json!({"queryLength": {"type": "integer", "minimum": 1, "maximum": 500}, "resultCount": bounded_count()})),
            object(&[], json!({"filterName": string_enum(&["resourceType", "applicableModel", "contentType", "catalogSearch", "family", "motorTechnology", "catalogFilters", "catalogPagination"]), "resultCount": bounded_count()})),
            object(&[], json!({"constraintCount": bounded_count(), "preferredFamily": string_enum(&["open", "centrifugal", "axial", "crossFlow", "inlineDuct", "motors"]), "priority": string_enum(&["efficiency", "noise", "size", "headroom"])})),
            object(&[], json!({"step": {"type": "integer", "minimum": 1, "maximum": 20}, "constraintCount": bounded_count()})),
            object(&[], json!({"outcome": string_enum(&["matched", "noValidatedCandidates", "engineeringReviewRequired"]), "candidateCount": bounded_count()})),
            object(&[], json!({"action": string_enum(&["add", "remove", "clear"]), "itemCount": {"type": "integer", "minimum": 0, "maximum": 4}, "productId": uuid(), "productRevision": revision()})),
            object(&[], json!({"downloadId": uuid(), "productId": uuid(), "productRevision": revision()})),
            object(&[], json!({"faqId": faq_id()})),
            object(&[], json!({"ctaId": string_enum(&["content-primary", "download-record-open", "request-quote"]), "placement": string_enum(&["content-panel", "downloads-list", "product-detail", "hero"])})),
            object(&[], json!({"journey": string_enum(&["product", "selection", "project", "replacement"]), "productId": uuid(), "productRevision": revision(), "step": {"type": "integer", "minimum": 1, "maximum": 20}, "fieldName": string_enum(&["productContext"]), "errorCode": string_enum(&["publishedContextRequired", "apiRejected"])}))
        ]
    })
}
