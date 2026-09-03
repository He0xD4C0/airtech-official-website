#[allow(clippy::too_many_arguments)]
async fn audit_mutation(
    state: &AppState,
    headers: &HeaderMap,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> Result<(), ApiError> {
    state
        .persist_audit(mutation_audit_event(
            headers,
            action,
            entity_type,
            entity_id,
            before,
            after,
            reason,
        ))
        .await
}

fn mutation_audit_event(
    headers: &HeaderMap,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    before: Option<Value>,
    after: Option<Value>,
    reason: Option<String>,
) -> AuditEvent {
    AuditEvent {
        id: Uuid::new_v4(),
        actor: actor(headers),
        action: action.into(),
        entity_type: entity_type.into(),
        entity_id,
        before,
        after,
        reason,
        request_id: request_id(headers),
        occurred_at: Utc::now(),
    }
}

async fn insert_audit_event_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &AuditEvent,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
    )
    .bind(event.id)
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.entity_type)
    .bind(event.entity_id)
    .bind(&event.before)
    .bind(&event.after)
    .bind(&event.reason)
    .bind(event.request_id)
    .bind(event.occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_memory_audit_if_needed(
    state: &AppState,
    event: AuditEvent,
) -> Result<(), ApiError> {
    if state.pool.is_none() {
        state.persist_audit(event).await?;
    }
    Ok(())
}
