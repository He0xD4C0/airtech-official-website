//! Development-only administrator provisioning for the local Compose stack.

use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    auth::{development_hash_password, development_validate_password},
    error::ApiError,
    models::AuditEvent,
    services::audit_log,
};

const PROVISIONING_LOCK: i64 = 672_183_921;

#[derive(Clone, Debug)]
pub struct DevelopmentAdminInput {
    pub display_name: String,
    pub email: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentAdminReport {
    pub status: &'static str,
}

pub async fn ensure(
    pool: &PgPool,
    input: &DevelopmentAdminInput,
) -> Result<DevelopmentAdminReport, ApiError> {
    validate(input)?;
    if users_exist(pool).await? {
        return Ok(report("skippedExistingUsers"));
    }
    let password_hash = development_hash_password(&input.password)?;
    let mut transaction = pool.begin().await?;
    lock(&mut transaction).await?;
    if users_exist_in(&mut transaction).await? {
        transaction.commit().await?;
        return Ok(report("skippedExistingUsers"));
    }

    let user_id = Uuid::new_v4();
    let role_id = ensure_super_admin_role(&mut transaction).await?;
    insert_user(&mut transaction, user_id, input, &password_hash).await?;
    assign_role(&mut transaction, user_id, role_id).await?;
    insert_audit(
        &mut transaction,
        user_id,
        "auth.development_admin_seeded",
        "Created the fixed local-development administrator in an empty database.",
    )
    .await?;
    transaction.commit().await?;
    Ok(report("created"))
}

pub async fn reset(
    pool: &PgPool,
    input: &DevelopmentAdminInput,
) -> Result<DevelopmentAdminReport, ApiError> {
    validate(input)?;
    let password_hash = development_hash_password(&input.password)?;
    let mut transaction = pool.begin().await?;
    lock(&mut transaction).await?;
    let existing =
        sqlx::query("SELECT id,status FROM users WHERE lower(email)=lower($1) FOR UPDATE")
            .bind(input.email.trim())
            .fetch_optional(&mut *transaction)
            .await?;
    let (user_id, previous_status) = match existing {
        Some(row) => (
            row.try_get("id")?,
            Some(row.try_get::<String, _>("status")?),
        ),
        None => (Uuid::new_v4(), None),
    };
    if existing_user(previous_status.as_ref()) {
        sqlx::query(
            r#"UPDATE users SET email=$1,password_hash=$2,display_name=$3,status='active',
               totp_secret_ciphertext=NULL,totp_confirmed_at=NULL,updated_at=now()
               WHERE id=$4"#,
        )
        .bind(normalized_email(input))
        .bind(&password_hash)
        .bind(input.display_name.trim())
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    } else {
        insert_user(&mut transaction, user_id, input, &password_hash).await?;
    }

    let role_id = ensure_super_admin_role(&mut transaction).await?;
    assign_role(&mut transaction, user_id, role_id).await?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id=$1")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE sessions SET revoked_at=now() WHERE user_id=$1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&mut *transaction)
        .await?;
    record_status_change(&mut transaction, user_id, previous_status.as_deref()).await?;
    insert_audit(
        &mut transaction,
        user_id,
        "auth.development_admin_reset",
        "Explicitly restored the fixed local-development administrator.",
    )
    .await?;
    transaction.commit().await?;
    Ok(report("reset"))
}

fn validate(input: &DevelopmentAdminInput) -> Result<(), ApiError> {
    let display_name = input.display_name.trim();
    let email = input.email.trim();
    if display_name.is_empty() || display_name.len() > 120 {
        return Err(ApiError::bad_request(
            "Development administrator display name must contain 1 to 120 characters.",
        ));
    }
    if !email.ends_with(".invalid") || !email.contains('@') {
        return Err(ApiError::bad_request(
            "Development administrator email must use the reserved .invalid domain.",
        ));
    }
    development_validate_password(&input.password)
}

async fn users_exist(pool: &PgPool) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(pool)
        .await?)
}

async fn users_exist_in(transaction: &mut Transaction<'_, Postgres>) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&mut **transaction)
        .await?)
}

async fn lock(transaction: &mut Transaction<'_, Postgres>) -> Result<(), ApiError> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(PROVISIONING_LOCK)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn ensure_super_admin_role(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Uuid, ApiError> {
    let role_id = sqlx::query_scalar(
        r#"INSERT INTO roles (id,key,display_name,system_role)
           VALUES ($1,'super-admin','Super Admin',true)
           ON CONFLICT (key) DO UPDATE SET display_name=EXCLUDED.display_name,system_role=true
           RETURNING id"#,
    )
    .bind(Uuid::new_v4())
    .fetch_one(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO role_permissions (role_id,permission_key) SELECT $1,key FROM permissions ON CONFLICT DO NOTHING",
    )
    .bind(role_id)
    .execute(&mut **transaction)
    .await?;
    Ok(role_id)
}

async fn insert_user(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    input: &DevelopmentAdminInput,
    password_hash: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO users
           (id,email,password_hash,display_name,status,created_at,updated_at)
           VALUES ($1,$2,$3,$4,'active',now(),now())"#,
    )
    .bind(user_id)
    .bind(normalized_email(input))
    .bind(password_hash)
    .bind(input.display_name.trim())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn assign_role(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    role_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO user_roles (user_id,role_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .bind(role_id)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn record_status_change(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    previous_status: Option<&str>,
) -> Result<(), ApiError> {
    if previous_status.is_some_and(|status| status != "active") {
        sqlx::query(
            r#"INSERT INTO user_status_history
               (id,user_id,from_status,to_status,reason,changed_by,request_id,changed_at)
               VALUES ($1,$2,$3,'active',$4,NULL,$5,now())"#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(previous_status)
        .bind("Explicit local development administrator reset.")
        .bind(Uuid::new_v4())
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn insert_audit(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    action: &str,
    reason: &str,
) -> Result<(), ApiError> {
    audit_log::insert_in_transaction(
        transaction,
        &AuditEvent {
            id: Uuid::new_v4(),
            actor: "development-maintenance".into(),
            action: action.into(),
            entity_type: "user".into(),
            entity_id: Some(user_id),
            before: None,
            after: Some(json!({"role": "Super Admin", "totpEnabled": false})),
            reason: Some(reason.into()),
            current_version: None,
            request_id: Uuid::new_v4(),
            occurred_at: Utc::now(),
        },
    )
    .await
}

fn normalized_email(input: &DevelopmentAdminInput) -> String {
    input.email.trim().to_ascii_lowercase()
}

fn existing_user(previous_status: Option<&String>) -> bool {
    previous_status.is_some()
}

fn report(status: &'static str) -> DevelopmentAdminReport {
    DevelopmentAdminReport { status }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(password: &str) -> DevelopmentAdminInput {
        DevelopmentAdminInput {
            display_name: "AIRTEK Local Administrator".into(),
            email: "local-admin@airtek.invalid".into(),
            password: password.into(),
        }
    }

    #[test]
    fn development_credentials_use_reserved_email_and_strong_password() {
        assert!(validate(&input("Airtek-Local-Admin-20260917!")).is_ok());
        assert!(validate(&input("too-short1")).is_err());
        let mut invalid = input("Airtek-Local-Admin-20260917!");
        invalid.email = "admin@example.com".into();
        assert!(validate(&invalid).is_err());
    }

    #[test]
    fn reports_never_serialize_credentials() {
        let serialized = serde_json::to_string(&report("created")).unwrap();
        assert_eq!(serialized, r#"{"status":"created"}"#);
        assert!(!serialized.contains("password"));
    }
}
