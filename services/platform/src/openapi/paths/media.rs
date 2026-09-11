//! Media upload, review, and immutable public delivery path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;

pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    let mut upload = admin(
        op(
            "uploadAdminMediaAsset",
            "Upload one PNG, JPEG, or WebP object into the review pipeline",
            "adminContent",
            [(
                "201",
                json_response("Media asset stored", r("MediaAssetSummary")),
            )],
        ),
        true,
    );
    upload["requestBody"] = json!({
        "required": true,
        "content": {
            "multipart/form-data": {
                "schema": object(
                    &["file"],
                    json!({
                        "file": {
                            "type": "string",
                            "format": "binary",
                            "description": "Single PNG, JPEG, or WebP image up to 25 MiB. SVG is rejected."
                        }
                    }),
                )
            }
        }
    });
    add(paths, "/api/admin/v1/media/uploads", "post", upload);

    add(
        paths,
        "/api/admin/v1/media/assets/{id}/scan",
        "post",
        admin(
            body(
                params(
                    op(
                        "reviewAdminMediaAsset",
                        "Record the human review decision for a media asset",
                        "adminContent",
                        [(
                            "200",
                            json_response("Media asset reviewed", r("MediaAssetSummary")),
                        )],
                    ),
                    vec![path_param("id", uuid())],
                ),
                r("MediaAssetReviewRequest"),
            ),
            true,
        ),
    );

    add(
        paths,
        "/api/public/v1/media/{assetId}",
        "get",
        params(
            op(
                "getPublicMediaAsset",
                "Serve one reviewed, public media object",
                "public",
                [(
                    "200",
                    text_response(
                        "Media object",
                        "image/png",
                        json!({"type": "string", "format": "binary"}),
                    ),
                )],
            ),
            vec![path_param("assetId", uuid())],
        ),
    );
    add(
        paths,
        "/api/public/v1/media/{assetId}/download",
        "get",
        params(
            op(
                "downloadPublicMediaAsset",
                "Download one reviewed, public media object as an attachment",
                "public",
                [(
                    "200",
                    text_response(
                        "Media attachment",
                        "application/octet-stream",
                        json!({"type": "string", "format": "binary"}),
                    ),
                )],
            ),
            vec![path_param("assetId", uuid())],
        ),
    );
}
