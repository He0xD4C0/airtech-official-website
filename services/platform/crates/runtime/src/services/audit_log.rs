use sqlx::{Postgres, Transaction};

use crate::error::ApiError;
use airtek_domain::models::AuditEvent;

pub async fn insert_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    event: &AuditEvent,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,current_version,request_id,occurred_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
    )
    .bind(event.id)
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.entity_type)
    .bind(event.entity_id)
    .bind(&event.before)
    .bind(&event.after)
    .bind(&event.reason)
    .bind(event.current_version)
    .bind(event.request_id)
    .bind(event.occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
