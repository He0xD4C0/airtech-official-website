//! Direct public media schemas.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add(schemas: &mut Map<String, Value>) {
    schemas.insert(
        "MediaAsset".into(),
        object(
            &[
                "id",
                "publicUrl",
                "downloadUrl",
                "originalName",
                "mediaType",
                "byteSize",
                "sha256",
                "uploadedBy",
                "createdAt",
            ],
            json!({
                "id": uuid(),
                "publicUrl": {"type": "string", "format": "uri", "pattern": "^https?://"},
                "downloadUrl": {"type": "string", "pattern": "^/api/public/v1/media/[0-9a-f-]+/download$"},
                "originalName": {"type": "string", "minLength": 1, "maxLength": 180},
                "mediaType": string_enum(&["image/png", "image/jpeg", "image/webp"]),
                "byteSize": {"type": "integer", "minimum": 1, "maximum": 26214400},
                "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                "uploadedBy": {"type": "string", "minLength": 1, "maxLength": 320},
                "createdAt": timestamp()
            }),
        ),
    );
    schemas.insert(
        "MediaAssetPage".into(),
        object(
            &["items", "nextCursor", "total"],
            json!({
                "items": array(r("MediaAsset")),
                "nextCursor": nullable(json!({"type": "string"})),
                "total": {"type": "integer", "minimum": 0}
            }),
        ),
    );
    schemas.insert(
        "MediaAssetReference".into(),
        object(
            &[
                "contentId",
                "contentRevision",
                "contentTitle",
                "contentStatus",
                "dependencyKind",
                "referencePath",
            ],
            json!({
                "contentId": uuid(),
                "contentRevision": revision(),
                "contentTitle": {"type": "string", "minLength": 1},
                "contentStatus": {"type": "string", "minLength": 1},
                "dependencyKind": string_enum(&["mediaInline", "mediaDownload"]),
                "referencePath": {"type": "string", "pattern": "^/"}
            }),
        ),
    );
    schemas.insert(
        "MediaAssetReferencePage".into(),
        page("MediaAssetReference"),
    );
    schemas.insert(
        "ResolvedMedia".into(),
        object(
            &[
                "assetId",
                "publicUrl",
                "downloadUrl",
                "mediaType",
                "byteSize",
                "originalName",
            ],
            json!({
                "assetId": uuid(),
                "publicUrl": {"type": "string", "format": "uri", "pattern": "^https?://"},
                "downloadUrl": {"type": "string", "pattern": "^/api/public/v1/media/"},
                "mediaType": string_enum(&["image/png", "image/jpeg", "image/webp"]),
                "byteSize": {"type": "integer", "minimum": 1},
                "originalName": {"type": "string", "minLength": 1}
            }),
        ),
    );

    schemas.insert(
        "ObjectStorageSettings".into(),
        object(
            &[
                "configured",
                "provider",
                "endpoint",
                "region",
                "bucket",
                "accessKeyId",
                "secretConfigured",
                "keyPrefix",
                "pathStyle",
                "publicBaseUrl",
                "legacyAssetCount",
                "revision",
                "updatedAt",
                "updatedBy",
            ],
            json!({
                "configured": {"type": "boolean"},
                "provider": {"type": "string", "const": "s3"},
                "endpoint": nullable(json!({"type": "string", "format": "uri"})),
                "region": nullable(json!({"type": "string"})),
                "bucket": nullable(json!({"type": "string"})),
                "accessKeyId": nullable(json!({"type": "string"})),
                "secretConfigured": {"type": "boolean", "readOnly": true},
                "keyPrefix": nullable(json!({"type": "string"})),
                "pathStyle": {"type": "boolean"},
                "publicBaseUrl": nullable(json!({"type": "string", "format": "uri"})),
                "legacyAssetCount": {"type": "integer", "minimum": 0, "readOnly": true},
                "revision": {"type": "integer", "minimum": 0, "readOnly": true},
                "updatedAt": nullable(timestamp()),
                "updatedBy": nullable(json!({"type": "string"}))
            }),
        ),
    );
    schemas.insert(
        "ObjectStorageSettingsInput".into(),
        object(
            &[
                "endpoint",
                "region",
                "bucket",
                "accessKeyId",
                "keyPrefix",
                "pathStyle",
                "publicBaseUrl",
            ],
            json!({
                "endpoint": {"type": "string", "format": "uri", "maxLength": 2048},
                "region": {"type": "string", "minLength": 1, "maxLength": 100},
                "bucket": {"type": "string", "minLength": 1, "maxLength": 255},
                "accessKeyId": {"type": "string", "minLength": 1, "maxLength": 512},
                "secretAccessKey": {"type": "string", "maxLength": 2048, "writeOnly": true},
                "keyPrefix": {"type": "string", "minLength": 1, "maxLength": 512},
                "pathStyle": {"type": "boolean"},
                "publicBaseUrl": {"type": "string", "format": "uri", "maxLength": 2048}
            }),
        ),
    );
    schemas.insert(
        "UpdateObjectStorageSettings".into(),
        json!({
            "allOf": [
                r("ObjectStorageSettingsInput"),
                object(
                    &["adoptLegacyAssets", "reason"],
                    json!({
                        "adoptLegacyAssets": {"type": "boolean"},
                        "reason": {"type": "string", "minLength": 12, "maxLength": 1000}
                    })
                )
            ]
        }),
    );
    schemas.insert(
        "ObjectStorageTestResult".into(),
        object(
            &["ok", "publicUrl"],
            json!({
                "ok": {"type": "boolean", "const": true},
                "publicUrl": {"type": "string", "format": "uri"}
            }),
        ),
    );
}
