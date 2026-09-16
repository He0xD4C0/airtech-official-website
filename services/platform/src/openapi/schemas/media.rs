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
                "publicUrl": {"type": "string", "pattern": "^/api/public/v1/media/[0-9a-f-]+$"},
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
                "publicUrl": {"type": "string", "pattern": "^/api/public/v1/media/"},
                "downloadUrl": {"type": "string", "pattern": "^/api/public/v1/media/"},
                "mediaType": string_enum(&["image/png", "image/jpeg", "image/webp"]),
                "byteSize": {"type": "integer", "minimum": 1},
                "originalName": {"type": "string", "minLength": 1}
            }),
        ),
    );
}
