//! Product import, presentation, user, role, and invitation schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

/// Adds management-platform data schemas.
pub(super) fn add(s: &mut Map<String, Value>) {
    s.insert(
        "RevisionRequest".into(),
        object(
            &["revision", "reason"],
            json!({
                "revision": revision(),
                "reason": {"type": "string", "minLength": 10}
            }),
        ),
    );
    s.insert(
        "ReasonRequest".into(),
        object(
            &["reason"],
            json!({"reason": {"type": "string", "minLength": 10}}),
        ),
    );

    s.insert(
        "ProductImportRequest".into(),
        object(
            &["csv"],
            json!({
                "csv": {"type": "string", "minLength": 1, "maxLength": 16777216, "writeOnly": true},
                "mappingVersion": nullable(json!({"type": "string", "minLength": 1, "maxLength": 120}))
            }),
        ),
    );
    s.insert(
        "ProductImportAccepted".into(),
        object(
            &["operationId", "status", "operationUrl", "eventsUrl"],
            json!({
                "operationId": uuid(),
                "status": string_enum(&["queued", "running", "completed", "failed", "cancelled"]),
                "operationUrl": {"type": "string", "pattern": "^/api/admin/v1/operations/"},
                "eventsUrl": {"type": "string", "pattern": "^/api/admin/v1/operations/"}
            }),
        ),
    );
    let product_import_row_error = object(
        &[
            "rowNumber",
            "stableId",
            "fieldName",
            "severity",
            "code",
            "detail",
        ],
        json!({
            "rowNumber": {"type": "integer", "minimum": 1},
            "stableId": nullable(json!({"type": "string"})),
            "fieldName": nullable(json!({"type": "string"})),
            "severity": string_enum(&["warning", "error"]),
            "code": {"type": "string"},
            "detail": {"type": "string"}
        }),
    );
    s.insert(
        "ProductImportRowError".into(),
        product_import_row_error.clone(),
    );
    // Compatibility name retained for existing Admin imports while new
    // consumers use the plan's explicit row-error contract name.
    s.insert("ProductImportError".into(), product_import_row_error);
    s.insert(
        "MissingAssetReference".into(),
        object(
            &["stableId", "assetType", "sourceReference"],
            json!({
                "stableId": {"type": "string"},
                "assetType": {"type": "string"},
                "sourceReference": {"type": "string"}
            }),
        ),
    );
    s.insert(
        "ProductImportResult".into(),
        object(
            &[
                "id",
                "checksum",
                "mappingVersion",
                "status",
                "totalRows",
                "validRows",
                "malformedRows",
                "errors",
                "missingAssets",
                "reused",
                "createdAt",
            ],
            json!({
                "id": uuid(),
                "checksum": {"type": "string", "pattern": "^[a-f0-9]{64}$"},
                "mappingVersion": {"type": "string"},
                "status": {"type": "string"},
                "totalRows": counter(),
                "validRows": counter(),
                "malformedRows": counter(),
                "errors": array(r("ProductImportRowError")),
                "missingAssets": array(r("MissingAssetReference")),
                "reused": {"type": "boolean"},
                "createdAt": timestamp()
            }),
        ),
    );
    s.insert(
        "ProductImportRun".into(),
        json!({"allOf": [r("ProductImportResult")]}),
    );
    s.insert(
        "ProductImportResultPage".into(),
        page("ProductImportResult"),
    );

    s.insert(
        "ProductPresentation".into(),
        object(
            &[
                "locale",
                "slug",
                "title",
                "summary",
                "seo",
                "indexable",
                "sortOrder",
                "relatedContentIds",
                "revision",
                "publishedRevision",
                "updatedAt",
            ],
            json!({
                "locale": {"type": "string"},
                "slug": slug(),
                "title": {"type": "string"},
                "summary": nullable(json!({"type": "string"})),
                "seo": r("SeoMetadata"),
                "indexable": {"type": "boolean"},
                "sortOrder": {"type": "integer"},
                "relatedContentIds": array(uuid()),
                "revision": revision(),
                "publishedRevision": nullable(revision()),
                "updatedAt": timestamp()
            }),
        ),
    );
    s.insert(
        "UpdateProductPresentation".into(),
        object(
            &["locale", "slug", "title", "seo", "indexable", "sortOrder", "relatedContentIds", "reason"],
            json!({
                "locale": {"type": "string"},
                "slug": slug(),
                "title": {"type": "string", "minLength": 1, "maxLength": 300},
                "summary": nullable(json!({"type": "string"})),
                "seo": r("SeoMetadataInput"),
                "indexable": {"type": "boolean"},
                "sortOrder": {"type": "integer", "default": 0},
                "relatedContentIds": {"type": "array", "maxItems": 100, "uniqueItems": true, "items": uuid(), "default": []},
                "reason": {"type": "string", "minLength": 10}
            }),
        ),
    );
    s.insert(
        "ProductPrivatePricing".into(),
        object(
            &["productId", "stableId", "sourceRowNumber", "pricingFields"],
            json!({
                "productId": uuid(),
                "stableId": {"type": "string"},
                "sourceRowNumber": {"type": "integer", "minimum": 1},
                "pricingFields": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "readOnly": true
                }
            }),
        ),
    );
    s.insert(
        "AdminProductDetail".into(),
        json!({
            "allOf": [
                r("Product"),
                object(
                    &["sourceKind", "missingAssets", "presentation"],
                    json!({
                        "sourceKind": r("DataClass"),
                        "missingAssets": array(r("MissingAssetReference")),
                        "presentation": nullable(r("ProductPresentation"))
                    })
                )
            ]
        }),
    );

    s.insert(
        "AdminUserRecord".into(),
        object(
            &[
                "id",
                "email",
                "displayName",
                "locale",
                "status",
                "revision",
                "managerUserId",
                "roles",
                "totpEnabled",
                "invitedAt",
                "lastLoginAt",
                "createdAt",
                "updatedAt",
            ],
            json!({
                "id": uuid(),
                "email": {"type": "string", "format": "email"},
                "displayName": {"type": "string"},
                "locale": {"type": "string"},
                "status": string_enum(&["invited", "active", "disabled"]),
                "revision": revision(),
                "managerUserId": nullable(uuid()),
                "roles": array(json!({"type": "string"})),
                "totpEnabled": {"type": "boolean"},
                "invitedAt": nullable(timestamp()),
                "lastLoginAt": nullable(timestamp()),
                "createdAt": timestamp(),
                "updatedAt": timestamp()
            }),
        ),
    );
    s.insert(
        "UserAdminSummary".into(),
        json!({"allOf": [r("AdminUserRecord")]}),
    );
    s.insert(
        "AdminUserRecordPage".into(),
        object(
            &["items", "nextCursor", "total"],
            json!({
                "items": array(r("AdminUserRecord")),
                "nextCursor": nullable(json!({"type": "string"})),
                "total": {"type": "integer", "minimum": 0}
            }),
        ),
    );
    s.insert(
        "UpdateAdminUser".into(),
        object(
            &["reason"],
            json!({
                "displayName": {"type": "string", "minLength": 1, "maxLength": 120},
                "locale": {"type": "string"},
                "status": string_enum(&["invited", "active", "disabled"]),
                "roleKeys": array(json!({"type": "string"})),
                "managerUserId": nullable(uuid()),
                "reason": {"type": "string", "minLength": 10}
            }),
        ),
    );
    s.insert(
        "AdminRoleRecord".into(),
        object(
            &[
                "id",
                "key",
                "displayName",
                "systemRole",
                "revision",
                "permissions",
            ],
            json!({
                "id": uuid(),
                "key": {"type": "string"},
                "displayName": {"type": "string"},
                "systemRole": {"type": "boolean"},
                "revision": revision(),
                "permissions": array(json!({"type": "string"}))
            }),
        ),
    );
    s.insert(
        "RoleDefinition".into(),
        json!({"allOf": [r("AdminRoleRecord")]}),
    );
    s.insert(
        "AdminRoleRecordPage".into(),
        object(
            &["items", "nextCursor", "total"],
            json!({
                "items": array(r("AdminRoleRecord")),
                "nextCursor": nullable(json!({"type": "string"})),
                "total": {"type": "integer", "minimum": 0}
            }),
        ),
    );
    s.insert(
        "UpdateAdminRole".into(),
        object(
            &["reason"],
            json!({
                "displayName": {"type": "string", "minLength": 1, "maxLength": 120},
                "permissions": array(json!({"type": "string"})),
                "reason": {"type": "string", "minLength": 10}
            }),
        ),
    );
    s.insert(
        "InviteAdminUser".into(),
        object(
            &["email", "displayName", "roleKeys"],
            json!({
                "email": {"type": "string", "format": "email"},
                "displayName": {"type": "string", "minLength": 1, "maxLength": 120},
                "roleKeys": {"type": "array", "minItems": 1, "items": {"type": "string"}}
            }),
        ),
    );
    s.insert(
        "UserInvitation".into(),
        object(
            &[
                "id",
                "email",
                "displayName",
                "locale",
                "roleKeys",
                "status",
                "invitedAt",
                "expiresAt",
            ],
            json!({
                "id": uuid(),
                "email": {"type": "string", "format": "email"},
                "displayName": {"type": "string"},
                "locale": {"type": "string"},
                "roleKeys": array(json!({"type": "string"})),
                "status": string_enum(&["pending", "accepted", "revoked", "expired"]),
                "invitedAt": timestamp(),
                "expiresAt": timestamp(),
                "invitationToken": {"type": "string", "readOnly": true}
            }),
        ),
    );
    s.insert("UserInvitationPage".into(), page("UserInvitation"));
}
