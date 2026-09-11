use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::MediaAssetSummary;
use crate::services::cms_content::require_postgres;
use crate::services::cms_preflight::stable_media_version_id;
use crate::state::AppState;

use super::config::MAX_MEDIA_UPLOAD_BYTES;
use super::storage;
use super::validate_storage_key;

/// The only media types the pipeline accepts. SVG is deliberately excluded: it
/// is executable content and needs a sanitizer this version does not ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcceptedMediaType {
    pub mime: &'static str,
    pub extension: &'static str,
}

const PNG: AcceptedMediaType = AcceptedMediaType {
    mime: "image/png",
    extension: "png",
};
const JPEG: AcceptedMediaType = AcceptedMediaType {
    mime: "image/jpeg",
    extension: "jpg",
};
const WEBP: AcceptedMediaType = AcceptedMediaType {
    mime: "image/webp",
    extension: "webp",
};

#[derive(Debug)]
pub struct ParsedUpload {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

pub fn sniff_media_type(bytes: &[u8]) -> Option<AcceptedMediaType> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some(PNG);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(JPEG);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(WEBP);
    }
    None
}

pub fn parse_upload_body(
    content_type: Option<&str>,
    body: &[u8],
) -> Result<ParsedUpload, ApiError> {
    let content_type = content_type.ok_or_else(unsupported_media_type)?;
    let boundary = multipart_boundary(content_type).ok_or_else(unsupported_media_type)?;
    if body.is_empty() {
        return Err(ApiError::bad_request("The upload body is empty."));
    }
    let delimiter = format!("--{boundary}");
    let mut cursor = 0_usize;
    while let Some(start) = find(&body[cursor..], delimiter.as_bytes()) {
        let after_delimiter = cursor + start + delimiter.len();
        let rest = &body[after_delimiter.min(body.len())..];
        if rest.starts_with(b"--") {
            break;
        }
        let part_start = rest.len() - rest.strip_prefix(b"\r\n").unwrap_or(rest).len();
        let remaining = &rest[part_start..];
        let Some(part_end) = find(remaining, delimiter.as_bytes()) else {
            break;
        };
        cursor = after_delimiter + part_start + part_end;
        let part = remaining[..part_end]
            .strip_suffix(b"\r\n")
            .unwrap_or(&remaining[..part_end]);
        let Some(header_end) = find(part, b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&part[..header_end]);
        let Some(disposition) = headers.lines().find(|line| {
            line.to_ascii_lowercase()
                .starts_with("content-disposition:")
        }) else {
            continue;
        };
        if parameter(disposition, "name").as_deref() != Some("file") {
            continue;
        }
        let file_name = parameter(disposition, "filename")
            .map(|name| sanitize_file_name(&name))
            .unwrap_or_else(|| "upload".to_owned());
        return Ok(ParsedUpload {
            file_name,
            bytes: part[header_end + 4..].to_vec(),
        });
    }
    Err(ApiError::bad_request(
        "The upload must contain a single `file` part.",
    ))
}

fn unsupported_media_type() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Unsupported media type",
        "Media uploads must be sent as multipart/form-data with a boundary.",
    )
}

fn multipart_boundary(content_type: &str) -> Option<String> {
    if !content_type
        .to_ascii_lowercase()
        .starts_with("multipart/form-data")
    {
        return None;
    }
    parameter(content_type, "boundary").filter(|value| {
        !value.is_empty()
            && value.len() <= 70
            && value
                .chars()
                .all(|character| character.is_ascii_graphic() && character != '"')
    })
}

fn parameter(header: &str, name: &str) -> Option<String> {
    let lowered = header.to_ascii_lowercase();
    let target = format!("{name}=");
    let mut search_from = 0_usize;
    let start = loop {
        let index = search_from + lowered[search_from..].find(&target)?;
        let starts_parameter = lowered[..index]
            .chars()
            .last()
            .is_none_or(|character| character == ';' || character.is_whitespace());
        if starts_parameter {
            break index + target.len();
        }
        search_from = index + target.len();
    };
    let value = header[start..].trim_start();
    if let Some(stripped) = value.strip_prefix('"') {
        let end = stripped.find('"')?;
        return Some(stripped[..end].to_owned());
    }
    Some(
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned(),
    )
}

fn sanitize_file_name(value: &str) -> String {
    let base = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(180)
        .collect::<String>();
    if base.is_empty() {
        "upload".to_owned()
    } else {
        base
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub async fn upload_media_asset(
    state: &AppState,
    actor: &str,
    request_id: Uuid,
    content_type: Option<&str>,
    body: &[u8],
) -> Result<MediaAssetSummary, ApiError> {
    let upload = parse_upload_body(content_type, body)?;
    ensure_upload_file_size(&upload.bytes)?;
    let media_type = sniff_media_type(&upload.bytes).ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Unsupported media type",
            "Only PNG, JPEG, and WebP images are accepted.",
        )
    })?;
    let settings = state.config.media.storage.as_ref().ok_or_else(|| {
        ApiError::service_unavailable(
            "Object storage is not configured; media uploads are disabled.",
        )
    })?;
    let asset_id = Uuid::new_v4();
    let storage_key = format!(
        "{}/{}/{}.{}",
        settings.key_prefix.trim_matches('/'),
        Utc::now().format("%Y/%m"),
        asset_id.simple(),
        media_type.extension
    );
    validate_storage_key(&storage_key)?;
    storage::put_object(
        settings,
        &storage_key,
        media_type.mime,
        upload.bytes.clone(),
    )
    .await?;

    let checksum = format!(
        "sha256:{}",
        data_encoding::HEXLOWER.encode(&Sha256::digest(&upload.bytes))
    );
    // Upload never creates a safety or editorial decision. A separate human
    // review is required before the public delivery gate can open.
    let scan_status = "pending";
    let persistence_result = async {
        let pool = require_postgres(state)?;
        let mut transaction = pool.begin().await?;
        let asset = insert_asset(
            &mut transaction,
            &AssetInsert {
                id: asset_id,
                storage_key: &storage_key,
                file_name: &upload.file_name,
                media_type: media_type.mime,
                byte_size: upload.bytes.len() as i64,
                checksum: &checksum,
                scan_status,
                storage_backend: settings.kind.label(),
            },
            actor,
            None,
        )
        .await?;
        insert_audit(
            &mut transaction,
            actor,
            request_id,
            "media.upload",
            asset_id,
            None,
            Some(json!({
                "storageKey": storage_key,
                "originalName": upload.file_name,
                "mediaType": media_type.mime,
                "byteSize": upload.bytes.len(),
                "checksum": checksum,
                "scanStatus": scan_status
            })),
            "Upload media object into the review pipeline",
        )
        .await?;
        transaction.commit().await?;
        Ok(asset)
    }
    .await;

    match persistence_result {
        Ok(asset) => Ok(asset),
        Err(error) => {
            if let Err(cleanup_error) = storage::delete_object(settings, &storage_key).await {
                tracing::error!(
                    %asset_id,
                    %storage_key,
                    persistence_error = %error,
                    cleanup_error = %cleanup_error,
                    "media upload compensation failed"
                );
            }
            Err(error)
        }
    }
}

fn ensure_upload_file_size(bytes: &[u8]) -> Result<(), ApiError> {
    if bytes.len() > MAX_MEDIA_UPLOAD_BYTES {
        Err(payload_too_large())
    } else {
        Ok(())
    }
}

struct AssetInsert<'a> {
    id: Uuid,
    storage_key: &'a str,
    file_name: &'a str,
    media_type: &'a str,
    byte_size: i64,
    checksum: &'a str,
    scan_status: &'a str,
    storage_backend: &'a str,
}

async fn insert_asset(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    asset: &AssetInsert<'_>,
    actor: &str,
    review_reason: Option<&str>,
) -> Result<MediaAssetSummary, ApiError> {
    let row = sqlx::query(
        "INSERT INTO media_assets (id,storage_key,original_name,media_type,byte_size,\
         checksum,scan_status,access_level,metadata,created_at,storage_backend,\
         content_type,uploaded_by,reviewed_by,reviewed_at,review_reason) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,'public','{}'::jsonb,now(),$8,$4,$9,\
         CASE WHEN $7='clean' THEN $9 ELSE NULL END,\
         CASE WHEN $7='clean' THEN now() ELSE NULL END,$10) \
         RETURNING id,original_name,media_type,byte_size,scan_status,access_level,created_at",
    )
    .bind(asset.id)
    .bind(asset.storage_key)
    .bind(asset.file_name)
    .bind(asset.media_type)
    .bind(asset.byte_size)
    .bind(asset.checksum)
    .bind(asset.scan_status)
    .bind(asset.storage_backend)
    .bind(actor)
    .bind(review_reason)
    .fetch_one(&mut **transaction)
    .await?;
    decode_asset_summary(&row)
}

pub async fn review_media_asset(
    state: &AppState,
    actor: &str,
    request_id: Uuid,
    asset_id: Uuid,
    status: &str,
    reason: Option<&str>,
) -> Result<MediaAssetSummary, ApiError> {
    let (status, reason) = human_review_decision(status, reason)?;
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    let before = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT jsonb_build_object('scanStatus',scan_status,'reviewReason',review_reason) \
         FROM media_assets WHERE id=$1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(asset_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(before) = before else {
        return Err(ApiError::not_found("Media asset was not found."));
    };
    sqlx::query(
        "UPDATE media_assets SET scan_status=$2,reviewed_by=$3,reviewed_at=now(),\
         review_reason=$4 WHERE id=$1 AND deleted_at IS NULL",
    )
    .bind(asset_id)
    .bind(status)
    .bind(actor)
    .bind(&reason)
    .execute(&mut *transaction)
    .await?;
    insert_audit(
        &mut transaction,
        actor,
        request_id,
        "media.review",
        asset_id,
        Some(before),
        Some(json!({"scanStatus": status, "reviewReason": reason})),
        "Record the human review decision for a media asset",
    )
    .await?;
    transaction.commit().await?;
    load_asset_summary(pool, asset_id).await
}

fn human_review_decision(
    status: &str,
    reason: Option<&str>,
) -> Result<(&'static str, String), ApiError> {
    let status = match status {
        "clean" => "clean",
        "quarantined" => "quarantined",
        _ => {
            return Err(ApiError::bad_request(
                "status must be clean or quarantined.",
            ))
        }
    };
    let reason = reason
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request(
                "A non-empty reason is required for every human media review decision.",
            )
        })?;
    if reason.chars().count() > 500 {
        return Err(ApiError::bad_request(
            "Media review reasons must be 500 characters or fewer.",
        ));
    }
    Ok((status, reason.to_owned()))
}

pub(super) async fn load_asset_summary(
    pool: &sqlx::PgPool,
    asset_id: Uuid,
) -> Result<MediaAssetSummary, ApiError> {
    let row = sqlx::query(
        "SELECT id,original_name,media_type,byte_size,scan_status,access_level,created_at \
         FROM media_assets WHERE id=$1 AND deleted_at IS NULL",
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Media asset was not found."))?;
    decode_asset_summary(&row)
}

fn decode_asset_summary(row: &sqlx::postgres::PgRow) -> Result<MediaAssetSummary, ApiError> {
    Ok(MediaAssetSummary {
        id: row.try_get("id")?,
        version_id: stable_media_version_id(row.try_get("id")?),
        original_name: row.try_get("original_name")?,
        media_type: row.try_get("media_type")?,
        byte_size: row.try_get("byte_size")?,
        scan_status: row.try_get("scan_status")?,
        access_level: row.try_get("access_level")?,
        created_at: row.try_get("created_at")?,
    })
}

fn payload_too_large() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
        "Payload too large",
        "Media uploads are limited to 25 MiB.",
    )
}

#[allow(clippy::too_many_arguments)]
async fn insert_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &str,
    request_id: Uuid,
    action: &str,
    entity_id: Uuid,
    before: Option<serde_json::Value>,
    after: Option<serde_json::Value>,
    reason: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO audit_log (id,actor,action,entity_type,entity_id,before_value,\
         after_value,reason,request_id,occurred_at) \
         VALUES ($1,$2,$3,'media',$4,$5,$6,$7,$8,$9)",
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(action)
    .bind(entity_id)
    .bind(before)
    .bind(after)
    .bind(reason)
    .bind(request_id)
    .bind(Utc::now())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
