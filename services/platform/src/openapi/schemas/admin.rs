//! Administrator authentication, sync, operations, settings, and analytics schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert("SetupRequest".into(), object(
        &["displayName", "email", "password", "bootstrapToken"],
        json!({"displayName": {"type": "string", "minLength": 1, "maxLength": 120}, "email": {"type": "string", "format": "email"}, "password": {"type": "string", "format": "password", "minLength": 12, "maxLength": 256, "writeOnly": true}, "bootstrapToken": {"type": "string", "writeOnly": true}})
    ));
    s.insert("LoginRequest".into(), json!({
        "type": "object", "additionalProperties": false, "required": ["email", "password"],
        "properties": {"email": {"type": "string", "format": "email"}, "password": {"type": "string", "format": "password", "writeOnly": true}, "otp": nullable(json!({"type": "string", "description": "A six-digit TOTP or one unused recovery code.", "pattern": "^(?:[0-9]{6}|[A-HJ-NP-Za-hj-np-z2-9]{4}(?:-[A-HJ-NP-Za-hj-np-z2-9]{4}){3})$", "writeOnly": true}))}
    }));
    s.insert(
        "AcceptInvitationRequest".into(),
        object(
            &["token", "password"],
            json!({
                "token": {"type": "string", "minLength": 43, "maxLength": 43, "pattern": "^[A-Za-z0-9_-]{43}$", "writeOnly": true},
                "password": {"type": "string", "format": "password", "minLength": 12, "maxLength": 256, "writeOnly": true}
            }),
        ),
    );
    s.insert(
        "InvitationAcceptance".into(),
        object(
            &[
                "userId",
                "email",
                "displayName",
                "locale",
                "roleKeys",
                "status",
                "acceptedAt",
            ],
            json!({
                "userId": uuid(),
                "email": {"type": "string", "format": "email"},
                "displayName": {"type": "string"},
                "locale": {"type": "string"},
                "roleKeys": array(json!({"type": "string"})),
                "status": {"type": "string", "const": "active"},
                "acceptedAt": timestamp()
            }),
        ),
    );
    s.insert("SessionUser".into(), object(
        &["id", "displayName", "email", "role", "permissions", "environment", "totpEnabled"],
        json!({"id": uuid(), "displayName": {"type": "string"}, "email": {"type": "string", "format": "email"}, "role": {"type": "string"}, "permissions": array(json!({"type": "string"})), "environment": {"type": "string"}, "totpEnabled": {"type": "boolean"}})
    ));
    s.insert(
        "TotpCodeRequest".into(),
        object(
            &["code"],
            json!({"code": {"type": "string", "pattern": "^[0-9]{6}$", "writeOnly": true}}),
        ),
    );
    s.insert(
        "TotpEnrollment".into(),
        object(
            &[
                "secret",
                "otpAuthUri",
                "algorithm",
                "digits",
                "periodSeconds",
            ],
            json!({
                "secret": {"type": "string", "pattern": "^[A-Z2-7]{32}$", "readOnly": true},
                "otpAuthUri": {"type": "string", "format": "uri", "readOnly": true},
                "algorithm": {"type": "string", "const": "SHA1"},
                "digits": {"type": "integer", "const": 6},
                "periodSeconds": {"type": "integer", "const": 30}
            }),
        ),
    );
    s.insert("RecoveryCodeSet".into(), object(
        &["recoveryCodes", "generatedAt"],
        json!({
            "recoveryCodes": {"type": "array", "minItems": 10, "maxItems": 10, "readOnly": true, "items": {"type": "string", "pattern": "^[A-HJ-NP-Z2-9]{4}(?:-[A-HJ-NP-Z2-9]{4}){3}$"}},
            "generatedAt": timestamp()
        })
    ));
    s.insert("AdminSession".into(), object(
        &["id", "current", "createdAt", "lastSeenAt", "expiresAt"],
        json!({"id": uuid(), "current": {"type": "boolean"}, "createdAt": timestamp(), "lastSeenAt": timestamp(), "expiresAt": timestamp()})
    ));
    s.insert(
        "SyncRunStatus".into(),
        string_enum(&[
            "queued",
            "fetching",
            "validating",
            "awaitingResolution",
            "readyToPublish",
            "completed",
            "completedWithErrors",
            "failed",
        ]),
    );
    s.insert("SyncRunKind".into(), string_enum(&["incremental", "full"]));
    s.insert(
        "StartSyncRequest".into(),
        json!({
            "type": "object", "additionalProperties": false, "required": ["runKind"],
            "properties": {"runKind": r("SyncRunKind")}
        }),
    );
    s.insert("SyncRun".into(), object(
        &["id", "source", "dryRun", "runKind", "mappingVersion", "status", "resumeCursor", "recordsSeen", "recordsValid", "conflictCount", "recordsApplied", "recordsFailed", "assetsSeen", "assetsCopied", "assetsReused", "assetsFailed", "startedAt", "completedAt"],
        json!({
            "id": uuid(), "connectorId": nullable(uuid()), "source": {"type": "string"}, "dryRun": {"type": "boolean"}, "runKind": r("SyncRunKind"), "mappingVersion": {"type": "string"}, "status": r("SyncRunStatus"),
            "resumeCursor": nullable(json!({"type": "string"})), "recordsSeen": counter(), "recordsValid": counter(), "conflictCount": counter(), "recordsApplied": counter(), "recordsFailed": counter(),
            "assetsSeen": counter(), "assetsCopied": counter(), "assetsReused": counter(), "assetsFailed": counter(),
            "startedAt": timestamp(), "completedAt": nullable(timestamp()), "error": nullable(json!({"type": "string"}))
        })
    ));
    s.insert("SyncRunPage".into(), page("SyncRun"));
    s.insert("FieldDiff".into(), object(
        &["fieldPath", "baseValue", "localValue", "incomingValue", "sourceOwned"],
        json!({"fieldPath": {"type": "string"}, "baseValue": nullable(json!({})), "localValue": nullable(json!({})), "incomingValue": nullable(json!({})), "sourceOwned": {"type": "boolean"}})
    ));
    s.insert("SyncConflict".into(), object(
        &["id", "syncRunId", "productId", "sourceRecordId", "diffs", "resolvedAt", "resolution", "revision"],
        json!({"id": uuid(), "syncRunId": uuid(), "productId": nullable(uuid()), "sourceRecordId": {"type": "string"}, "diffs": array(r("FieldDiff")), "resolvedAt": nullable(timestamp()), "resolution": nullable(json!({"type": "string"})), "revision": revision()})
    ));
    s.insert("SyncConflictPage".into(), object(
        &["items", "nextCursor", "total"],
        json!({"items": array(r("SyncConflict")), "nextCursor": nullable(json!({"type": "string"})), "total": {"type": "integer", "minimum": 0}})
    ));
    s.insert(
        "StagingValidationStatus".into(),
        string_enum(&["pending", "valid", "invalid", "conflicted"]),
    );
    s.insert("StagingRecord".into(), object(
        &["id", "syncRunId", "sourceSnapshotId", "sourceRecordId", "validationStatus", "normalizedPayload", "validationErrors", "createdAt"],
        json!({"id": uuid(), "syncRunId": uuid(), "sourceSnapshotId": uuid(), "sourceRecordId": {"type": "string"}, "validationStatus": r("StagingValidationStatus"), "normalizedPayload": nullable(json!({})), "validationErrors": array(r("ValidationIssue")), "createdAt": timestamp()})
    ));
    s.insert("StagingRecordPage".into(), object(
        &["items", "nextCursor", "total"],
        json!({"items": array(r("StagingRecord")), "nextCursor": nullable(json!({"type": "string"})), "total": {"type": "integer", "minimum": 0}})
    ));
    s.insert("FeishuConnectionStatus".into(), object(
        &["connectorId", "displayName", "configured", "enabled", "runnable", "unavailableReason", "updatedAt", "latestSync"],
        json!({"connectorId": nullable(uuid()), "displayName": nullable(json!({"type": "string"})), "configured": {"type": "boolean"}, "enabled": {"type": "boolean"}, "runnable": {"type": "boolean"}, "unavailableReason": nullable(json!({"type": "string"})), "updatedAt": nullable(timestamp()), "latestSync": nullable(r("SyncRun"))})
    ));
    s.insert("FeishuSource".into(), object(
        &["wikiToken", "tableId", "name", "family", "application"],
        json!({"wikiToken": {"type": "string"}, "tableId": {"type": "string"}, "name": {"type": "string"}, "family": r("ProductFamily"), "application": nullable(json!({"type": "string"}))})
    ));
    s.insert("FeishuSettings".into(), object(
        &["connectorId", "enabled", "intervalMinutes", "fullReconcileEnabled", "fullReconcileLocalTime", "timezone", "mappingVersion", "sources", "revision", "lastIncrementalAt", "lastFullAt", "updatedAt", "updatedBy"],
        json!({
            "connectorId": uuid(), "enabled": {"type": "boolean"}, "intervalMinutes": {"type": "integer", "minimum": 5, "maximum": 1440},
            "fullReconcileEnabled": {"type": "boolean"}, "fullReconcileLocalTime": {"type": "string", "pattern": "^(?:[01][0-9]|2[0-3]):[0-5][0-9]$"},
            "timezone": {"type": "string", "const": "Asia/Shanghai"}, "mappingVersion": {"type": "string", "const": "feishu-product-v1"},
            "sources": array(r("FeishuSource")), "revision": revision(), "lastIncrementalAt": nullable(timestamp()), "lastFullAt": nullable(timestamp()),
            "updatedAt": timestamp(), "updatedBy": {"type": "string"}
        })
    ));
    s.insert("UpdateFeishuSettings".into(), object(
        &["enabled", "intervalMinutes", "fullReconcileEnabled", "fullReconcileLocalTime"],
        json!({"enabled": {"type": "boolean"}, "intervalMinutes": {"type": "integer", "minimum": 5, "maximum": 1440}, "fullReconcileEnabled": {"type": "boolean"}, "fullReconcileLocalTime": {"type": "string", "pattern": "^(?:[01][0-9]|2[0-3]):[0-5][0-9]$"}})
    ));
    s.insert("FeishuTableCheck".into(), object(
        &["tableId", "name", "accessible", "fieldCount", "mappingValid", "errors"],
        json!({"tableId": {"type": "string"}, "name": {"type": "string"}, "accessible": {"type": "boolean"}, "fieldCount": counter(), "mappingValid": {"type": "boolean"}, "errors": array(json!({"type": "string"}))})
    ));
    s.insert("FeishuConnectionTest".into(), object(
        &["credentialsConfigured", "tokenIssued", "objectStorageReady", "runnable", "tables", "checkedAt"],
        json!({"credentialsConfigured": {"type": "boolean"}, "tokenIssued": {"type": "boolean"}, "objectStorageReady": {"type": "boolean"}, "runnable": {"type": "boolean"}, "tables": array(r("FeishuTableCheck")), "checkedAt": timestamp()})
    ));
    s.insert("FeishuSyncError".into(), object(
        &["sourceRecordId", "code", "fieldPath", "message", "createdAt"],
        json!({"sourceRecordId": nullable(json!({"type": "string"})), "code": {"type": "string"}, "fieldPath": nullable(json!({"type": "string"})), "message": {"type": "string"}, "createdAt": timestamp()})
    ));
    s.insert(
        "FeishuSyncRunDetail".into(),
        object(
            &["run", "errors"],
            json!({"run": r("SyncRun"), "errors": array(r("FeishuSyncError"))}),
        ),
    );
    s.insert("FeishuRollbackReport".into(), object(
        &["syncRunId", "restored", "skipped", "completedAt"],
        json!({"syncRunId": uuid(), "restored": counter(), "skipped": counter(), "completedAt": timestamp()})
    ));
    s.insert("SyncMapping".into(), object(
        &["id", "connectorId", "version", "mapping", "schemaVersion", "active", "createdAt"],
        json!({"id": uuid(), "connectorId": uuid(), "version": {"type": "string"}, "mapping": {}, "schemaVersion": {"type": "integer"}, "active": {"type": "boolean"}, "createdAt": timestamp()})
    ));
    s.insert("SyncMappingPage".into(), page("SyncMapping"));
    s.insert(
        "SyncConflictDecision".into(),
        string_enum(&["acceptIncoming", "keepVerifiedLocal"]),
    );
    s.insert("ResolveSyncConflictRequest".into(), object(
        &["decision", "evidenceReference", "reason", "expiresAt"],
        json!({"decision": r("SyncConflictDecision"), "evidenceReference": nullable(json!({"type": "string", "minLength": 1})), "reason": {"type": "string", "minLength": 10, "maxLength": 2000}, "expiresAt": nullable(timestamp())})
    ));
    s.insert("CreateTemporaryOverride".into(), object(
        &["productId", "fieldPath", "value", "reason"],
        json!({"productId": uuid(), "fieldPath": {"type": "string", "minLength": 1}, "value": {}, "reason": {"type": "string", "minLength": 10}, "expiresAt": nullable(timestamp())})
    ));
    s.insert("TemporaryOverride".into(), object(
        &["id", "productId", "fieldPath", "value", "reason", "createdAt", "expiresAt", "expired"],
        json!({"id": uuid(), "productId": uuid(), "fieldPath": {"type": "string"}, "value": {}, "reason": {"type": "string"}, "createdAt": timestamp(), "expiresAt": timestamp(), "expired": {"type": "boolean"}})
    ));
    s.insert("TemporaryOverridePage".into(), page("TemporaryOverride"));
    s.insert("OperationKind".into(), string_enum(&["productImport"]));
    s.insert(
        "OperationStatus".into(),
        string_enum(&["queued", "running", "completed", "failed"]),
    );
    s.insert("BackgroundOperation".into(), object(
        &["id", "kind", "status", "reason", "createdAt", "updatedAt", "result"],
        json!({"id": uuid(), "kind": r("OperationKind"), "status": r("OperationStatus"), "reason": {"type": "string"}, "createdAt": timestamp(), "updatedAt": timestamp(), "result": nullable(json!({}))})
    ));
    s.insert(
        "PlatformSettings".into(),
        object(
            &[
                "rfqRetentionDays",
                "retentionDeletionGraceDays",
                "temporaryOverrideDefaultDays",
                "publicLocale",
                "revision",
            ],
            json!({
                "rfqRetentionDays": {"type": "integer", "minimum": 30, "maximum": 3650},
                "retentionDeletionGraceDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "temporaryOverrideDefaultDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "publicLocale": {"type": "string", "const": "en", "readOnly": true},
                "revision": {"type": "integer", "minimum": 1, "readOnly": true}
            }),
        ),
    );
    s.insert(
        "UpdatePlatformSettings".into(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["reason"],
            "minProperties": 2,
            "properties": {
                "rfqRetentionDays": {"type": "integer", "minimum": 30, "maximum": 3650},
                "retentionDeletionGraceDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "temporaryOverrideDefaultDays": {"type": "integer", "minimum": 1, "maximum": 365},
                "reason": {"type": "string", "minLength": 12}
            }
        }),
    );
    s.insert("AuditEvent".into(), object(
        &["id", "actor", "action", "entityType", "entityId", "before", "after", "reason", "currentVersion", "requestId", "occurredAt"],
        json!({"id": uuid(), "actor": {"type": "string"}, "action": {"type": "string"}, "entityType": {"type": "string"}, "entityId": nullable(uuid()), "before": nullable(json!({})), "after": nullable(json!({})), "reason": nullable(json!({"type": "string"})), "currentVersion": nullable(json!({"type": "integer", "minimum": 0})), "requestId": uuid(), "occurredAt": timestamp()})
    ));
    s.insert("AuditEventPage".into(), object(
        &["items", "nextCursor", "total"],
        json!({"items": array(r("AuditEvent")), "nextCursor": nullable(json!({"type": "string"})), "total": {"type": "integer", "minimum": 0}})
    ));
    s.insert(
        "AnalyticsOverviewRange".into(),
        object(
            &["from", "toExclusive", "timezone"],
            json!({
                "from": timestamp(),
                "toExclusive": timestamp(),
                "timezone": {"type": "string", "const": "UTC"}
            }),
        ),
    );
    s.insert(
        "AnalyticsConsentedMetrics".into(),
        object(
            &[
                "visits",
                "pageViews",
                "engagedVisitDays",
                "rfqStartEvents",
                "rfqSubmitEvents",
            ],
            json!({
                "visits": counter(),
                "pageViews": counter(),
                "engagedVisitDays": counter(),
                "rfqStartEvents": counter(),
                "rfqSubmitEvents": counter()
            }),
        ),
    );
    s.insert(
        "AnalyticsBusinessOutcomes".into(),
        object(
            &["rfqSubmissions", "contactRequests"],
            json!({
                "rfqSubmissions": counter(),
                "contactRequests": counter()
            }),
        ),
    );
    s.insert("DashboardMetric".into(), object(
        &["available", "value", "unavailableReason"],
        json!({"available": {"type": "boolean"}, "value": nullable(counter()), "unavailableReason": nullable(json!({"type": "string"}))})
    ));
    s.insert("AdminDashboardSummary".into(), object(
        &["generatedAt", "draftContent", "openConflicts", "openRfqs", "analytics", "recentActivity", "readinessItemCount"],
        json!({
            "generatedAt": timestamp(), "draftContent": r("DashboardMetric"), "openConflicts": r("DashboardMetric"),
            "openRfqs": r("DashboardMetric"),
            "analytics": nullable(r("AnalyticsConsentedMetrics")), "recentActivity": array(r("AuditEvent")),
            "readinessItemCount": counter()
        })
    ));
    s.insert(
        "AnalyticsOverview".into(),
        object(
            &[
                "range",
                "generatedAt",
                "consentedMetrics",
                "businessOutcomes",
                "source",
                "containsPii",
            ],
            json!({
                "range": r("AnalyticsOverviewRange"),
                "generatedAt": timestamp(),
                "consentedMetrics": r("AnalyticsConsentedMetrics"),
                "businessOutcomes": r("AnalyticsBusinessOutcomes"),
                "source": {"type": "string", "const": "firstParty"},
                "containsPii": {"type": "boolean", "const": false}
            }),
        ),
    );
    #[cfg(feature = "devtools")]
    s.insert("TerminalToken".into(), object(&["token", "expiresInSeconds"], json!({"token": {"type": "string"}, "expiresInSeconds": {"type": "integer", "minimum": 1}})));
}
