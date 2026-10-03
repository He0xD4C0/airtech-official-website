//! First-run administrator provisioning and recovery-key reconciliation.
//!
//! The `.env` administrator credentials are applied only to an empty database
//! or to the one-time rename of the reserved development account. An
//! initialized database ignores the configured password entirely.

use chrono::Utc;
use serde::Serialize;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::auth::{hash_secret, validate_password_strength, verify_secret};
use crate::config::Config;
use crate::error::ApiError;
use crate::services::{audit_log, recovery_key};

use airtek_domain::models::AuditEvent;

const PROVISIONING_LOCK: i64 = 672_183_921;
pub const LEGACY_ADMIN_EMAIL: &str = "local-admin@airtek.invalid";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminProvisioningReport {
    pub status: &'static str,
    pub recovery_key_origin: Option<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyRotation {
    pub status: &'static str,
    pub email: String,
    pub path: String,
    /// Printed once by the operator command so the key can be stored offline.
    pub recovery_key: String,
}

/// Explicit rotation for an operator who replaced or lost the key file. The
/// application never rotates a key on startup; only this command and a
/// successful recovery-key login do.
pub async fn rotate_recovery_key(
    pool: &PgPool,
    config: &Config,
) -> Result<RecoveryKeyRotation, ApiError> {
    let email = config.admin_email.as_deref().ok_or_else(|| {
        ApiError::service_unavailable("AIRTEK_ADMIN_EMAIL is required to rotate the recovery key.")
    })?;
    let dir = config.admin_recovery_key_dir.as_deref().ok_or_else(|| {
        ApiError::service_unavailable(
            "AIRTEK_ADMIN_RECOVERY_KEY_DIR is required to rotate the recovery key.",
        )
    })?;
    let mut transaction = pool.begin().await?;
    let user_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE lower(email)=lower($1) AND status='active' FOR UPDATE",
    )
    .bind(email)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("No active administrator matches AIRTEK_ADMIN_EMAIL."))?;
    let resolved = recovery_key::rotate(dir)?;
    let hash = hash_secret(&resolved.key)?;
    sqlx::query(
        r#"UPDATE users SET recovery_key_hash=$2, recovery_key_origin=$3,
             recovery_key_confirmed_at=NULL, recovery_key_rotated_at=now(), updated_at=now()
           WHERE id=$1"#,
    )
    .bind(user_id)
    .bind(hash)
    .bind(resolved.origin)
    .execute(&mut *transaction)
    .await?;
    audit(
        &mut transaction,
        "auth.admin.recoveryKeyRotated",
        Some(user_id),
        serde_json::json!({"origin": resolved.origin}),
    )
    .await?;
    transaction.commit().await?;
    Ok(RecoveryKeyRotation {
        status: "rotated",
        email: email.to_owned(),
        path: recovery_key::recovery_key_path(dir).display().to_string(),
        recovery_key: resolved.key,
    })
}

pub async fn ensure(pool: &PgPool, config: &Config) -> Result<AdminProvisioningReport, ApiError> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(PROVISIONING_LOCK)
        .execute(&mut *transaction)
        .await?;
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&mut *transaction)
        .await?;

    let (status, origin) = if users == 0 {
        let origin = create_initial_admin(&mut transaction, config).await?;
        audit(
            &mut transaction,
            "auth.admin.provisioned",
            None,
            serde_json::json!({"status": "created", "recoveryKeyOrigin": origin}),
        )
        .await?;
        ("created", origin)
    } else {
        let renamed = rename_legacy_admin(&mut transaction, config).await?;
        if renamed {
            audit(
                &mut transaction,
                "auth.admin.legacyRenamed",
                None,
                serde_json::json!({"status": "renamedLegacy"}),
            )
            .await?;
        }
        let origin = reconcile_recovery_key(&mut transaction, config).await?;
        (if renamed { "renamedLegacy" } else { "existing" }, origin)
    };
    ensure_active_super_admin(&mut transaction).await?;
    transaction.commit().await?;
    Ok(AdminProvisioningReport {
        status,
        recovery_key_origin: origin,
    })
}

async fn create_initial_admin(
    transaction: &mut Transaction<'_, Postgres>,
    config: &Config,
) -> Result<Option<&'static str>, ApiError> {
    let email = config
        .admin_email
        .as_deref()
        .ok_or_else(missing_credentials)?;
    let display_name = config
        .admin_display_name
        .as_deref()
        .ok_or_else(missing_credentials)?;
    let password = config
        .admin_password
        .as_deref()
        .ok_or_else(missing_credentials)?;
    if !valid_email(email) || display_name.trim().is_empty() || display_name.len() > 120 {
        return Err(ApiError::service_unavailable(
            "AIRTEK_ADMIN_EMAIL or AIRTEK_ADMIN_DISPLAY_NAME is invalid.",
        ));
    }
    validate_password_strength(password).map_err(|_| {
        ApiError::service_unavailable(
            "AIRTEK_ADMIN_PASSWORD must contain 12 to 256 characters with a letter and a digit.",
        )
    })?;
    let password_hash = hash_secret(password)?;
    let user_id = Uuid::new_v4();
    let role_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO roles (id, key, display_name, system_role, is_preset)
           VALUES ($1,'super-admin','Super Admin',true,true)
           ON CONFLICT (key) DO UPDATE SET display_name=EXCLUDED.display_name,
             system_role=true, is_preset=true
           RETURNING id"#,
    )
    .bind(Uuid::new_v4())
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO role_permissions (role_id, permission_key)
         SELECT $1, key FROM permissions ON CONFLICT DO NOTHING",
    )
    .bind(role_id)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO users
             (id,email,password_hash,display_name,status,must_change_password,created_at,updated_at)
           VALUES ($1,$2,$3,$4,'active',true,now(),now())"#,
    )
    .bind(user_id)
    .bind(email)
    .bind(&password_hash)
    .bind(display_name.trim())
    .execute(&mut **transaction)
    .await?;
    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1,$2)")
        .bind(user_id)
        .bind(role_id)
        .execute(&mut **transaction)
        .await?;
    let origin = configure_recovery_key(transaction, config, user_id, None).await?;
    Ok(origin)
}

/// Renames the reserved development account to the configured address exactly
/// once, preserving its identity and role assignments. A conflicting target
/// aborts startup instead of merging two administrators silently.
async fn rename_legacy_admin(
    transaction: &mut Transaction<'_, Postgres>,
    config: &Config,
) -> Result<bool, ApiError> {
    let Some(target_email) = config.admin_email.as_deref() else {
        return Ok(false);
    };
    if target_email.eq_ignore_ascii_case(LEGACY_ADMIN_EMAIL) {
        return Ok(false);
    }
    let legacy = sqlx::query("SELECT id FROM users WHERE lower(email)=lower($1) FOR UPDATE")
        .bind(LEGACY_ADMIN_EMAIL)
        .fetch_optional(&mut **transaction)
        .await?;
    let Some(legacy) = legacy else {
        return Ok(false);
    };
    let legacy_id: Uuid = legacy.try_get("id")?;
    let conflict: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE lower(email)=lower($1) AND id<>$2)",
    )
    .bind(target_email)
    .bind(legacy_id)
    .fetch_one(&mut **transaction)
    .await?;
    if conflict {
        return Err(ApiError::service_unavailable(
            "AIRTEK_ADMIN_EMAIL already belongs to another administrator; resolve it before starting.",
        ));
    }
    let password_hash = match config.admin_password.as_deref() {
        Some(password) => {
            validate_password_strength(password).map_err(|_| {
                ApiError::service_unavailable(
                    "AIRTEK_ADMIN_PASSWORD must contain 12 to 256 characters with a letter and a digit.",
                )
            })?;
            Some(hash_secret(password)?)
        }
        None => None,
    };
    sqlx::query(
        r#"UPDATE users SET email=$2, password_hash=COALESCE($3,password_hash),
             must_change_password=true, status='active', updated_at=now()
           WHERE id=$1"#,
    )
    .bind(legacy_id)
    .bind(target_email)
    .bind(password_hash.as_deref())
    .execute(&mut **transaction)
    .await?;
    sqlx::query("UPDATE sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL")
        .bind(legacy_id)
        .execute(&mut **transaction)
        .await?;
    Ok(true)
}

async fn reconcile_recovery_key(
    transaction: &mut Transaction<'_, Postgres>,
    config: &Config,
) -> Result<Option<&'static str>, ApiError> {
    let Some(email) = config.admin_email.as_deref() else {
        return Ok(None);
    };
    let row = sqlx::query("SELECT id, recovery_key_hash FROM users WHERE lower(email)=lower($1)")
        .bind(email)
        .fetch_optional(&mut **transaction)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let user_id: Uuid = row.try_get("id")?;
    let existing: Option<String> = row.try_get("recovery_key_hash")?;
    configure_recovery_key(transaction, config, user_id, existing.as_deref()).await
}

async fn configure_recovery_key(
    transaction: &mut Transaction<'_, Postgres>,
    config: &Config,
    user_id: Uuid,
    existing_hash: Option<&str>,
) -> Result<Option<&'static str>, ApiError> {
    let dir = config.admin_recovery_key_dir.as_deref().ok_or_else(|| {
        ApiError::service_unavailable(
            "AIRTEK_ADMIN_RECOVERY_KEY_DIR is required to manage the administrator recovery key.",
        )
    })?;
    match existing_hash {
        Some(hash) => {
            let key = recovery_key::read_existing(dir)?.ok_or_else(|| {
                ApiError::service_unavailable(
                    "The administrator recovery key file is missing but a key hash is stored.",
                )
            })?;
            if !verify_secret(hash, &key) {
                return Err(ApiError::service_unavailable(
                    "The administrator recovery key file does not match the stored hash; restore the original file or rotate it explicitly.",
                ));
            }
            let origin: Option<String> =
                sqlx::query_scalar("SELECT recovery_key_origin FROM users WHERE id=$1")
                    .bind(user_id)
                    .fetch_one(&mut **transaction)
                    .await?;
            Ok(Some(
                if origin.as_deref() == Some(recovery_key::GENERATED_ORIGIN) {
                    recovery_key::GENERATED_ORIGIN
                } else {
                    recovery_key::PROVIDED_ORIGIN
                },
            ))
        }
        None => {
            let resolved = recovery_key::resolve(dir, config.admin_recovery_key_mode)?;
            let hash = hash_secret(&resolved.key)?;
            sqlx::query(
                r#"UPDATE users SET recovery_key_hash=$2, recovery_key_origin=$3,
                     recovery_key_confirmed_at=NULL, recovery_key_rotated_at=now(), updated_at=now()
                   WHERE id=$1"#,
            )
            .bind(user_id)
            .bind(hash)
            .bind(resolved.origin)
            .execute(&mut **transaction)
            .await?;
            Ok(Some(resolved.origin))
        }
    }
}

async fn ensure_active_super_admin(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<(), ApiError> {
    let active: i64 = sqlx::query_scalar(
        r#"SELECT count(DISTINCT account.id) FROM users account
           JOIN user_roles assignment ON assignment.user_id=account.id
           JOIN roles role ON role.id=assignment.role_id
           WHERE account.status='active' AND role.key='super-admin'"#,
    )
    .fetch_one(&mut **transaction)
    .await?;
    if active == 0 {
        return Err(ApiError::service_unavailable(
            "No active Super Admin exists. Re-activate one directly in PostgreSQL before starting the platform.",
        ));
    }
    Ok(())
}

async fn audit(
    transaction: &mut Transaction<'_, Postgres>,
    action: &str,
    entity_id: Option<Uuid>,
    after: serde_json::Value,
) -> Result<(), ApiError> {
    let event = AuditEvent {
        id: Uuid::new_v4(),
        actor: "system:admin-provisioning".into(),
        action: action.into(),
        entity_type: "user".into(),
        entity_id,
        before: None,
        after: Some(after),
        reason: None,
        current_version: None,
        request_id: Uuid::new_v4(),
        occurred_at: Utc::now(),
    };
    audit_log::insert_in_transaction(transaction, &event).await
}

fn missing_credentials() -> ApiError {
    ApiError::service_unavailable(
        "An empty database requires AIRTEK_ADMIN_EMAIL, AIRTEK_ADMIN_DISPLAY_NAME and AIRTEK_ADMIN_PASSWORD.",
    )
}

fn valid_email(value: &str) -> bool {
    value.len() <= 254
        && !value.contains(':')
        && !value.chars().any(char::is_whitespace)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
}
