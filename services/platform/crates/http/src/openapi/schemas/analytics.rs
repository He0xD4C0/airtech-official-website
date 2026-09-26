//! Privacy-minimized first-party visit and source-report schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

/// Adds consented visit and aggregate-report schemas.
pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "CreateGuestVisit".into(),
        object(
            &[
                "anonymousSessionId",
                "consentReceipt",
                "policyVersion",
                "landingPath",
            ],
            json!({
                "anonymousSessionId": uuid(),
                "consentReceipt": uuid(),
                "policyVersion": r("AnalyticsPolicyVersion"),
                "landingPath": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 2048,
                    "pattern": "^/en(?:/[^?#]*)?$"
                },
                "referrerDomain": nullable(json!({
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 253,
                    "pattern": "^[^/\\\\?#@]+$"
                })),
                "source": nullable(json!({
                    "type": "string", "maxLength": 128,
                    "pattern": "^[A-Za-z0-9][A-Za-z0-9._:-]*$",
                    "description": "Accepted only when registered in the deployment UTM source allowlist."
                })),
                "medium": nullable(json!({
                    "type": "string", "maxLength": 128,
                    "pattern": "^[A-Za-z0-9][A-Za-z0-9._:-]*$",
                    "description": "Accepted only when registered in the deployment UTM medium allowlist."
                })),
                "campaign": nullable(json!({
                    "type": "string", "maxLength": 200,
                    "pattern": "^[A-Za-z0-9][A-Za-z0-9._:-]*$",
                    "description": "Accepted only when registered in the deployment UTM campaign allowlist."
                }))
            }),
        ),
    );
    s.insert(
        "GuestVisit".into(),
        object(
            &[
                "id",
                "anonymousSessionId",
                "landingPath",
                "referrerDomain",
                "source",
                "medium",
                "campaign",
                "firstSeenAt",
                "lastSeenAt",
                "retentionUntil",
            ],
            json!({
                "id": uuid(),
                "anonymousSessionId": uuid(),
                "landingPath": {"type": "string"},
                "referrerDomain": nullable(json!({"type": "string"})),
                "source": {"type": "string"},
                "medium": nullable(json!({"type": "string"})),
                "campaign": nullable(json!({"type": "string"})),
                "firstSeenAt": timestamp(),
                "lastSeenAt": timestamp(),
                "retentionUntil": timestamp()
            }),
        ),
    );
    s.insert("GuestVisitPage".into(), page("GuestVisit"));
    s.insert(
        "GuestSourceDaily".into(),
        object(
            &[
                "bucketDate",
                "source",
                "sourceName",
                "referrerDomain",
                "utmSource",
                "medium",
                "campaign",
                "landingPath",
                "locale",
                "visits",
                "pageViews",
                "rfqStarts",
                "rfqSubmissions",
            ],
            json!({
                "bucketDate": {"type": "string", "format": "date"},
                "source": {"type": "string"},
                "sourceName": nullable(json!({"type": "string"})),
                "referrerDomain": nullable(json!({"type": "string"})),
                "utmSource": nullable(json!({"type": "string"})),
                "medium": nullable(json!({"type": "string"})),
                "campaign": nullable(json!({"type": "string"})),
                "landingPath": {"type": "string", "pattern": "^/en(?:/|$)[^?#]*$"},
                "locale": {"type": "string"},
                "visits": counter(),
                "pageViews": counter(),
                "rfqStarts": counter(),
                "rfqSubmissions": counter()
            }),
        ),
    );
    s.insert(
        "GuestSourceSummary".into(),
        json!({"allOf": [r("GuestSourceDaily")]}),
    );
    s.insert("GuestSourceDailyPage".into(), page("GuestSourceDaily"));
}
