//! Read-only CMS migration-preflight schemas.

use serde_json::{json, Map, Value};

use super::super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "MigrationPreflightSeverity".into(),
        string_enum(&["warning", "blocking"]),
    );
    s.insert(
        "MigrationPreflightSource".into(),
        string_enum(&[
            "content",
            "contentRevision",
            "news",
            "generalInformation",
            "media",
            "relation",
            "route",
        ]),
    );
    s.insert(
        "MigrationPreflightIssueCode".into(),
        string_enum(&[
            "invalidLegacyPayload",
            "unsupportedContentKind",
            "unsupportedTemplate",
            "embeddedPageSlots",
            "unknownBlock",
            "invalidTiptapDocument",
            "typeFieldMismatch",
            "invalidRelation",
            "missingRelationTarget",
            "relationHistoryUnavailable",
            "missingMediaAsset",
            "missingMediaVersion",
            "duplicatePublicPath",
            "missingPublicRoute",
            "routeOwnershipMismatch",
            "canonicalPathMismatch",
            "scheduledPublicationUnsupported",
            "revisionConversionFailed",
        ]),
    );
    s.insert(
        "MigrationPreflightIssue".into(),
        object(
            &["severity", "code", "source", "message"],
            json!({
                "severity": r("MigrationPreflightSeverity"),
                "code": r("MigrationPreflightIssueCode"),
                "source": r("MigrationPreflightSource"),
                "entityId": nullable(uuid()),
                "revision": nullable(revision()),
                "jsonPath": nullable(json!({"type": "string", "maxLength": 1000})),
                "message": {"type": "string", "minLength": 1, "maxLength": 4000}
            }),
        ),
    );
    s.insert(
        "MigrationPreflightCounts".into(),
        object(
            &[
                "contentEntries",
                "contentRevisions",
                "newsEntries",
                "generalInformationEntries",
                "mediaAssets",
                "mediaReferences",
                "relations",
                "publicRoutes",
            ],
            json!({
                "contentEntries": counter(),
                "contentRevisions": counter(),
                "newsEntries": counter(),
                "generalInformationEntries": counter(),
                "mediaAssets": counter(),
                "mediaReferences": counter(),
                "relations": counter(),
                "publicRoutes": counter()
            }),
        ),
    );
    s.insert(
        "MigrationPreflightReport".into(),
        object(
            &[
                "targetSchemaVersion",
                "generatedAt",
                "canMigrate",
                "scanned",
                "convertible",
                "blockingIssueCount",
                "warningCount",
                "issues",
            ],
            json!({
                "targetSchemaVersion": {"type": "integer", "const": 2},
                "generatedAt": timestamp(),
                "canMigrate": {"type": "boolean"},
                "scanned": r("MigrationPreflightCounts"),
                "convertible": r("MigrationPreflightCounts"),
                "blockingIssueCount": counter(),
                "warningCount": counter(),
                "issues": {"type": "array", "items": r("MigrationPreflightIssue")}
            }),
        ),
    );
}
