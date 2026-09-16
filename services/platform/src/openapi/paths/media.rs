//! Synchronous media upload, catalogue detail, references, and public bytes.

use serde_json::{json, Map, Value};

use super::super::support::*;
use crate::error::{MEDIA_DECODE_FAILED, MEDIA_IDEMPOTENCY_CONFLICT};

pub(super) fn add_paths(paths: &mut Map<String, Value>) {
    add_upload(paths);
    add_admin_reads(paths);
    add_public_reads(paths);
}

fn add_upload(paths: &mut Map<String, Value>) {
    let mut operation = admin(
        params(
            op(
                "uploadAdminMediaAsset",
                "Upload one PNG, JPEG, or WebP asset and make its API URL public immediately",
                "adminContent",
                [("201", json_response("Public media asset", r("MediaAsset")))],
            ),
            vec![idempotency_param()],
        ),
        true,
    );
    operation["requestBody"] = json!({
        "required": true,
        "content": {
            "multipart/form-data": {
                "schema": object(
                    &["file"],
                    json!({
                        "file": {
                            "type": "string",
                            "format": "binary",
                            "description": "Exactly one PNG, JPEG, or WebP file up to 25 MiB."
                        }
                    })
                )
            }
        }
    });
    operation["responses"]["413"] = problem_response("The file exceeds 25 MiB");
    operation = with_problem_example(
        operation,
        "409",
        MEDIA_IDEMPOTENCY_CONFLICT,
        "Idempotency key conflict",
        "This user already used the key for different file bytes.",
    );
    operation = with_problem_example(
        operation,
        "415",
        MEDIA_DECODE_FAILED,
        "Unsupported media bytes",
        "The file header is not PNG, JPEG, or WebP.",
    );
    add(paths, "/api/admin/v1/media/assets", "post", operation);
}

fn add_admin_reads(paths: &mut Map<String, Value>) {
    add(
        paths,
        "/api/admin/v1/media/assets/{id}",
        "get",
        admin(
            params(
                op(
                    "getAdminMediaAsset",
                    "Get one public media asset",
                    "adminContent",
                    [("200", json_response("Media asset", r("MediaAsset")))],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    let mut parameters = admin_pagination_params();
    parameters.insert(0, path_param("id", uuid()));
    add(
        paths,
        "/api/admin/v1/media/assets/{id}/references",
        "get",
        admin(
            params(
                op(
                    "listAdminMediaAssetReferences",
                    "List CMS publication snapshots that reference a media asset",
                    "adminContent",
                    [(
                        "200",
                        json_response("Media references", r("MediaAssetReferencePage")),
                    )],
                ),
                parameters,
            ),
            false,
        ),
    );
}

fn add_public_reads(paths: &mut Map<String, Value>) {
    for (path, operation_id, summary) in [
        (
            "/api/public/v1/media/{assetId}",
            "getPublicMediaAsset",
            "Serve a live media asset without authentication",
        ),
        (
            "/api/public/v1/media/{assetId}/download",
            "downloadPublicMediaAsset",
            "Download a live media asset without authentication",
        ),
    ] {
        add(
            paths,
            path,
            "get",
            params(
                op(
                    operation_id,
                    summary,
                    "public",
                    [(
                        "200",
                        text_response(
                            "Media bytes",
                            "application/octet-stream",
                            json!({"type": "string", "format": "binary"}),
                        ),
                    )],
                ),
                vec![path_param("assetId", uuid())],
            ),
        );
    }
}
