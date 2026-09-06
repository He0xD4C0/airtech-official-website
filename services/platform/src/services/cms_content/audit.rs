use super::*;

pub(super) async fn insert_audit(
    transaction: &mut Transaction<'_, Postgres>,
    metadata: &MutationMetadata,
    action: &str,
    entity_id: Uuid,
    before: Option<Value>,
    after: Option<Value>,
    reason: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,$3,'content',$4,$5,$6,$7,$8,$9)"#,
    )
    .bind(Uuid::new_v4())
    .bind(&metadata.actor)
    .bind(action)
    .bind(entity_id)
    .bind(before)
    .bind(after)
    .bind(reason)
    .bind(metadata.request_id)
    .bind(Utc::now())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
