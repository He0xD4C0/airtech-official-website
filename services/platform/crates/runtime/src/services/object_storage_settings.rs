use std::{collections::BTreeMap, path::PathBuf};

use chrono::Utc;
use serde_json::json;
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::media::{self, MediaStorageKind, MediaStorageSettings};
use crate::state::AppState;
use airtek_domain::models::{
    ObjectStorageSettings, ObjectStorageSettingsInput, ObjectStorageTestResult,
    UpdateObjectStorageSettings,
};

const SETTINGS_LOCK_KEY: &str = "airtek.object-storage-settings";

pub async fn get(state: &AppState) -> Result<ObjectStorageSettings, ApiError> {
    let legacy_asset_count = legacy_asset_count(&state.pool).await?;
    let row = sqlx::query(
        r#"SELECT endpoint,region,bucket,access_key_id,key_prefix,path_style,
                  public_base_url,revision,updated_at,updated_by
           FROM object_storage_settings WHERE singleton=true"#,
    )
    .fetch_optional(&state.pool)
    .await?;
    row.as_ref()
        .map(|row| decode_public(row, legacy_asset_count))
        .unwrap_or_else(|| Ok(ObjectStorageSettings::unconfigured(legacy_asset_count)))
}

pub async fn active_storage(state: &AppState) -> Result<MediaStorageSettings, ApiError> {
    #[cfg(any(test, feature = "devtools"))]
    if let Some(settings) = &state.config.media.storage {
        return Ok(settings.clone());
    }
    let row = sqlx::query(
        r#"SELECT endpoint,region,bucket,access_key_id,secret_access_key,
                  key_prefix,path_style,public_base_url
           FROM object_storage_settings WHERE singleton=true"#,
    )
    .fetch_optional(&state.pool)
    .await?;
    if let Some(row) = row {
        return decode_storage(&row);
    }
    Err(ApiError::service_unavailable(
        "Object storage is not configured; media uploads are disabled.",
    ))
}

pub async fn test(
    state: &AppState,
    input: &ObjectStorageSettingsInput,
) -> Result<ObjectStorageTestResult, ApiError> {
    let settings = validated_storage(state, input).await?;
    let key = format!(
        "{}/.airtek-probe/{}.txt",
        settings.key_prefix,
        Uuid::new_v4().simple()
    );
    let public_url = public_url(&settings, &key);
    media::probe_storage(&settings, &key, &public_url).await?;
    Ok(ObjectStorageTestResult {
        ok: true,
        public_url,
    })
}

pub async fn update(
    state: &AppState,
    expected_revision: i64,
    update: &UpdateObjectStorageSettings,
    actor: &str,
    request_id: Uuid,
) -> Result<ObjectStorageSettings, ApiError> {
    validate_reason(&update.reason)?;
    let observed_revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT revision FROM object_storage_settings WHERE singleton=true),0)",
    )
    .fetch_one(&state.pool)
    .await?;
    if observed_revision != expected_revision {
        return Err(ApiError::conflict(
            "The object storage settings changed; reload before saving.",
        ));
    }
    let settings = validated_storage(state, &update.settings).await?;
    let probe_key = format!(
        "{}/.airtek-probe/{}.txt",
        settings.key_prefix,
        Uuid::new_v4().simple()
    );
    media::probe_storage(&settings, &probe_key, &public_url(&settings, &probe_key)).await?;

    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(SETTINGS_LOCK_KEY)
        .execute(&mut *transaction)
        .await?;
    let current = sqlx::query(
        r#"SELECT endpoint,region,bucket,access_key_id,key_prefix,path_style,
                  public_base_url,revision,updated_at,updated_by
           FROM object_storage_settings WHERE singleton=true FOR UPDATE"#,
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let current_revision = current
        .as_ref()
        .map(|row| row.try_get::<i64, _>("revision"))
        .transpose()?
        .unwrap_or(0);
    if current_revision != expected_revision {
        return Err(ApiError::conflict(
            "The object storage settings changed; reload before saving.",
        ));
    }
    let legacy_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM media_assets WHERE deleted_at IS NULL AND public_url IS NULL",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if legacy_count > 0 && !update.adopt_legacy_assets {
        let mut errors = BTreeMap::new();
        errors.insert(
            "adoptLegacyAssets".into(),
            vec![format!(
                "Confirm adoption of {legacy_count} legacy media assets before saving."
            )],
        );
        return Err(ApiError::validation(errors));
    }

    let revision = current_revision + 1;
    let now = Utc::now();
    sqlx::query(
        r#"INSERT INTO object_storage_settings
           (singleton,provider,endpoint,region,bucket,access_key_id,secret_access_key,
            key_prefix,path_style,public_base_url,revision,updated_at,updated_by)
           VALUES (true,'s3',$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           ON CONFLICT (singleton) DO UPDATE SET
             endpoint=EXCLUDED.endpoint,region=EXCLUDED.region,bucket=EXCLUDED.bucket,
             access_key_id=EXCLUDED.access_key_id,secret_access_key=EXCLUDED.secret_access_key,
             key_prefix=EXCLUDED.key_prefix,path_style=EXCLUDED.path_style,
             public_base_url=EXCLUDED.public_base_url,revision=EXCLUDED.revision,
             updated_at=EXCLUDED.updated_at,updated_by=EXCLUDED.updated_by"#,
    )
    .bind(&settings.endpoint)
    .bind(&settings.region)
    .bind(&settings.bucket)
    .bind(&settings.access_key_id)
    .bind(&settings.secret_access_key)
    .bind(&settings.key_prefix)
    .bind(settings.path_style)
    .bind(&settings.public_base_url)
    .bind(revision)
    .bind(now)
    .bind(actor)
    .execute(&mut *transaction)
    .await?;
    if legacy_count > 0 {
        sqlx::query(
            r#"UPDATE media_assets SET public_url=$1 || '/' || storage_key
               WHERE deleted_at IS NULL AND public_url IS NULL"#,
        )
        .bind(&settings.public_base_url)
        .execute(&mut *transaction)
        .await?;
    }
    let before = current
        .as_ref()
        .map(|row| decode_public(row, legacy_count))
        .transpose()?;
    let after = public_settings(&settings, revision, now, actor.to_owned(), 0);
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,
            current_version,request_id,occurred_at)
           VALUES ($1,$2,'settings.objectStorage.update','objectStorageSettings',NULL,
                   $3,$4,$5,NULL,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(before.map(|value| json!(value)))
    .bind(json!(after))
    .bind(update.reason.trim())
    .bind(request_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(after)
}

pub fn public_url(settings: &MediaStorageSettings, key: &str) -> String {
    format!("{}/{}", settings.public_base_url.trim_end_matches('/'), key)
}

async fn validated_storage(
    state: &AppState,
    input: &ObjectStorageSettingsInput,
) -> Result<MediaStorageSettings, ApiError> {
    let secret = if input.secret_access_key.trim().is_empty() {
        sqlx::query_scalar::<_, String>(
            "SELECT secret_access_key FROM object_storage_settings WHERE singleton=true",
        )
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| validation("secretAccessKey", "Secret access key is required."))?
    } else {
        input.secret_access_key.clone()
    };
    validate_input(input, &secret)?;
    Ok(MediaStorageSettings {
        kind: MediaStorageKind::S3,
        local_root: PathBuf::from(media::DEFAULT_LOCAL_MEDIA_ROOT),
        endpoint: input.endpoint.trim().to_owned(),
        region: input.region.trim().to_owned(),
        bucket: input.bucket.trim().to_owned(),
        access_key_id: input.access_key_id.trim().to_owned(),
        secret_access_key: secret,
        key_prefix: input.key_prefix.trim_matches('/').to_owned(),
        path_style: input.path_style,
        public_base_url: input.public_base_url.trim_end_matches('/').to_owned(),
    })
}

fn validate_input(input: &ObjectStorageSettingsInput, secret: &str) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    for (field, value, max) in [
        ("endpoint", input.endpoint.trim(), 2048),
        ("region", input.region.trim(), 100),
        ("bucket", input.bucket.trim(), 255),
        ("accessKeyId", input.access_key_id.trim(), 512),
        ("secretAccessKey", secret, 2048),
        ("keyPrefix", input.key_prefix.trim_matches('/'), 512),
        ("publicBaseUrl", input.public_base_url.trim(), 2048),
    ] {
        if value.is_empty() || value.len() > max {
            errors.insert(
                field.into(),
                vec![format!("Value must contain 1 to {max} bytes.")],
            );
        }
    }
    for (field, value) in [
        ("endpoint", input.endpoint.trim()),
        ("publicBaseUrl", input.public_base_url.trim()),
    ] {
        if !valid_http_url(value) {
            errors.insert(
                field.into(),
                vec!["Use an absolute http:// or https:// URL without query or fragment.".into()],
            );
        }
    }
    let prefix = input.key_prefix.trim_matches('/');
    if prefix
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        errors.insert(
            "keyPrefix".into(),
            vec!["Use a safe relative key prefix.".into()],
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn valid_http_url(value: &str) -> bool {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"));
    rest.is_some_and(|rest| {
        !rest.is_empty()
            && !rest.starts_with('/')
            && !value.ends_with('/')
            && !value.contains(['?', '#'])
            && !value.chars().any(char::is_whitespace)
    })
}

fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if (12..=1000).contains(&reason.trim().chars().count()) {
        Ok(())
    } else {
        Err(validation(
            "reason",
            "Reason must contain 12 to 1000 characters.",
        ))
    }
}

fn validation(field: &str, detail: &str) -> ApiError {
    let mut errors = BTreeMap::new();
    errors.insert(field.into(), vec![detail.into()]);
    ApiError::validation(errors)
}

async fn legacy_asset_count(pool: &sqlx::PgPool) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM media_assets WHERE deleted_at IS NULL AND public_url IS NULL",
    )
    .fetch_one(pool)
    .await?)
}

fn decode_storage(row: &PgRow) -> Result<MediaStorageSettings, ApiError> {
    Ok(MediaStorageSettings {
        kind: MediaStorageKind::S3,
        local_root: PathBuf::from(media::DEFAULT_LOCAL_MEDIA_ROOT),
        endpoint: row.try_get("endpoint")?,
        region: row.try_get("region")?,
        bucket: row.try_get("bucket")?,
        access_key_id: row.try_get("access_key_id")?,
        secret_access_key: row.try_get("secret_access_key")?,
        key_prefix: row.try_get("key_prefix")?,
        path_style: row.try_get("path_style")?,
        public_base_url: row.try_get("public_base_url")?,
    })
}

fn decode_public(row: &PgRow, legacy_asset_count: i64) -> Result<ObjectStorageSettings, ApiError> {
    Ok(ObjectStorageSettings {
        configured: true,
        provider: "s3".into(),
        endpoint: Some(row.try_get("endpoint")?),
        region: Some(row.try_get("region")?),
        bucket: Some(row.try_get("bucket")?),
        access_key_id: Some(row.try_get("access_key_id")?),
        secret_configured: true,
        key_prefix: Some(row.try_get("key_prefix")?),
        path_style: row.try_get("path_style")?,
        public_base_url: Some(row.try_get("public_base_url")?),
        legacy_asset_count,
        revision: row.try_get("revision")?,
        updated_at: Some(row.try_get("updated_at")?),
        updated_by: Some(row.try_get("updated_by")?),
    })
}

fn public_settings(
    settings: &MediaStorageSettings,
    revision: i64,
    updated_at: chrono::DateTime<Utc>,
    updated_by: String,
    legacy_asset_count: i64,
) -> ObjectStorageSettings {
    ObjectStorageSettings {
        configured: true,
        provider: "s3".into(),
        endpoint: Some(settings.endpoint.clone()),
        region: Some(settings.region.clone()),
        bucket: Some(settings.bucket.clone()),
        access_key_id: Some(settings.access_key_id.clone()),
        secret_configured: true,
        key_prefix: Some(settings.key_prefix.clone()),
        path_style: settings.path_style,
        public_base_url: Some(settings.public_base_url.clone()),
        legacy_asset_count,
        revision,
        updated_at: Some(updated_at),
        updated_by: Some(updated_by),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_urls_are_built_from_the_saved_base_and_key() {
        let settings = MediaStorageSettings {
            kind: MediaStorageKind::S3,
            local_root: PathBuf::from(media::DEFAULT_LOCAL_MEDIA_ROOT),
            endpoint: "https://s3.example.test".into(),
            region: "test-1".into(),
            bucket: "assets".into(),
            access_key_id: "access".into(),
            secret_access_key: "secret".into(),
            key_prefix: "media".into(),
            path_style: true,
            public_base_url: "https://cdn.example.test/assets".into(),
        };
        assert_eq!(
            public_url(&settings, "media/2026/photo.png"),
            "https://cdn.example.test/assets/media/2026/photo.png"
        );
    }

    #[test]
    fn public_settings_serialization_contains_no_secret_material() {
        let serialized = serde_json::to_string(&ObjectStorageSettings::unconfigured(3)).unwrap();
        assert!(serialized.contains("\"secretConfigured\":false"));
        assert!(!serialized.contains("secretAccessKey"));
    }

    #[test]
    fn public_urls_reject_queries_fragments_and_trailing_slashes() {
        assert!(valid_http_url("https://cdn.example.test/media"));
        assert!(!valid_http_url("https://cdn.example.test/media/"));
        assert!(!valid_http_url(
            "https://cdn.example.test/media?token=secret"
        ));
        assert!(!valid_http_url("cdn.example.test/media"));
    }
}
