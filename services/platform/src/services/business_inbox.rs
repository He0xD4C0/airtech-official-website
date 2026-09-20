use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        BusinessEntityType, BusinessInboxDetail, BusinessInboxItem, BusinessInboxPage,
        BusinessInboxStatus, BusinessPii,
    },
    pagination::{
        cursor_limit, decode_scoped_cursor_compat, encode_scoped_cursor, CursorQuery, DecodedCursor,
    },
    services::request_metrics::LegacyCursorEndpoint,
    state::AppState,
};

#[path = "business_inbox/support.rs"]
mod support;
use support::*;

#[derive(Clone, Debug, Default)]
pub struct InboxFilter {
    pub query: Option<String>,
    pub status: Option<BusinessInboxStatus>,
    pub assigned_to: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize)]
struct InboxCursor {
    updated_at: DateTime<Utc>,
    id: Uuid,
}

pub async fn list(
    state: &AppState,
    entity_type: BusinessEntityType,
    filter: &InboxFilter,
    pagination: CursorQuery,
) -> Result<BusinessInboxPage, ApiError> {
    let pool = &state.pool;
    let table = table(entity_type);
    let limit = cursor_limit(&pagination)?;
    let scope = format!(
        "admin.business.{}|{:?}|{:?}|{:?}",
        entity_type.label(),
        filter.query,
        filter.status,
        filter.assigned_to
    );
    let after = match pagination.cursor.as_deref() {
        Some(value) => Some(resolve_cursor(state, entity_type, filter, &scope, value).await?),
        None => None,
    };
    let count_sql = format!(
        r#"SELECT count(*) FROM {table}
           WHERE ($1::text IS NULL OR (CASE WHEN payload->>'status'='piiCleared' THEN 'piiCleared' ELSE status END)=$1)
             AND ($2::uuid IS NULL OR assigned_to=$2)
             AND ($3::text IS NULL OR reference ILIKE '%' || $3 || '%' OR payload::text ILIKE '%' || $3 || '%')"#,
    );
    let total = sqlx::query_scalar::<_, i64>(&count_sql)
        .bind(filter.status.map(BusinessInboxStatus::label))
        .bind(filter.assigned_to)
        .bind(filter.query.as_deref())
        .fetch_one(pool)
        .await?;
    let sql = format!(
        r#"SELECT id,reference,status,revision,assigned_to,submitted_at,updated_at,
                  retention_until,source_path,locale,payload
           FROM {table}
           WHERE ($1::text IS NULL OR (CASE WHEN payload->>'status'='piiCleared' THEN 'piiCleared' ELSE status END)=$1)
             AND ($2::uuid IS NULL OR assigned_to=$2)
             AND ($3::text IS NULL OR reference ILIKE '%' || $3 || '%' OR payload::text ILIKE '%' || $3 || '%')
             AND ($4::timestamptz IS NULL OR (updated_at,id)<($4,$5))
           ORDER BY updated_at DESC,id DESC LIMIT $6"#,
    );
    let rows = sqlx::query(&sql)
        .bind(filter.status.map(BusinessInboxStatus::label))
        .bind(filter.assigned_to)
        .bind(filter.query.as_deref())
        .bind(after.as_ref().map(|value| value.updated_at))
        .bind(after.as_ref().map(|value| value.id))
        .bind(i64::try_from(limit + 1).unwrap_or(101))
        .fetch_all(pool)
        .await?;
    let mut items = rows
        .into_iter()
        .map(|row| item_from_row(entity_type, row))
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = if has_more {
        items
            .last()
            .map(|item| {
                encode_scoped_cursor(
                    &scope,
                    &InboxCursor {
                        updated_at: item.updated_at,
                        id: item.id,
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(BusinessInboxPage {
        items,
        next_cursor,
        total: usize::try_from(total).unwrap_or(usize::MAX),
    })
}

async fn resolve_cursor(
    state: &AppState,
    entity_type: BusinessEntityType,
    filter: &InboxFilter,
    scope: &str,
    value: &str,
) -> Result<InboxCursor, ApiError> {
    match decode_scoped_cursor_compat::<Uuid, InboxCursor>(scope, value)? {
        DecodedCursor::Current(cursor) => Ok(cursor),
        DecodedCursor::Legacy(id) => {
            let sql = format!(
                r#"SELECT updated_at FROM {} WHERE id=$1
                   AND ($2::text IS NULL OR (CASE WHEN payload->>'status'='piiCleared' THEN 'piiCleared' ELSE status END)=$2)
                   AND ($3::uuid IS NULL OR assigned_to=$3)
                   AND ($4::text IS NULL OR reference ILIKE '%' || $4 || '%' OR payload::text ILIKE '%' || $4 || '%')"#,
                table(entity_type),
            );
            let updated_at = sqlx::query_scalar::<_, DateTime<Utc>>(&sql)
                .bind(id)
                .bind(filter.status.map(BusinessInboxStatus::label))
                .bind(filter.assigned_to)
                .bind(filter.query.as_deref())
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::BusinessInbox);
            Ok(InboxCursor { updated_at, id })
        }
    }
}

pub async fn get(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
) -> Result<BusinessInboxItem, ApiError> {
    let pool = require_pool(state)?;
    let sql = format!(
        r#"SELECT id,reference,status,revision,assigned_to,submitted_at,updated_at,
                  retention_until,source_path,locale,payload
           FROM {} WHERE id=$1"#,
        table(entity_type),
    );
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Business inbox item was not found."))?;
    item_from_row(entity_type, row)
}

pub async fn detail(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
) -> Result<BusinessInboxDetail, ApiError> {
    let item = get(state, entity_type, id).await?;
    let pool = require_pool(state)?;
    let notes = sqlx::query(
        r#"SELECT id,entity_type,entity_id,body,created_by,created_at
           FROM business_internal_notes
           WHERE entity_type=$1 AND entity_id=$2 ORDER BY created_at DESC,id DESC"#,
    )
    .bind(entity_type.label())
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(note_from_row)
    .collect::<Result<Vec<_>, _>>()?;
    let status_history = sqlx::query(
        r#"SELECT id,from_status,to_status,note,changed_by,changed_at
           FROM business_status_history
           WHERE entity_type=$1 AND entity_id=$2 ORDER BY changed_at DESC,id DESC"#,
    )
    .bind(entity_type.label())
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(status_history_from_row)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(BusinessInboxDetail {
        item,
        notes,
        status_history,
    })
}

pub async fn pii(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
) -> Result<BusinessPii, ApiError> {
    let pool = require_pool(state)?;
    let sql = format!(
        "SELECT payload FROM {} WHERE id=$1 AND payload->>'status' IS DISTINCT FROM 'piiCleared'",
        table(entity_type)
    );
    let payload = sqlx::query_scalar::<_, Value>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("Business inbox item was not found."))?;
    match entity_type {
        BusinessEntityType::Rfq => {
            let request = decode_rfq(payload)?.request;
            let mut detail = pii_from_contact(request.contact, None);
            if !request.context.is_empty() {
                detail.rfq_context = Some(
                    serde_json::from_value(serde_json::json!({
                        "journey": request.journey, "context": request.context
                    }))
                    .map_err(|_| ApiError::service_unavailable("Stored RFQ context is invalid."))?,
                );
            }
            Ok(detail)
        }
        BusinessEntityType::Contact => {
            let request = decode_contact(payload)?.request;
            let message = request.message.clone();
            Ok(pii_from_contact(request.contact, Some(message)))
        }
    }
}

pub async fn assign(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
    expected: i64,
    assigned_to: Option<Uuid>,
    reason: &str,
    actor: &str,
) -> Result<BusinessInboxItem, ApiError> {
    validate_reason(reason)?;
    let pool = require_pool(state)?;
    if let Some(user_id) = assigned_to {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND status='active')",
        )
        .bind(user_id)
        .fetch_one(pool)
        .await?;
        if !exists {
            return Err(ApiError::validation(std::collections::BTreeMap::from([(
                "assignedTo".into(),
                vec!["Assignee must be an active Admin user.".into()],
            )])));
        }
    }
    let mut transaction = pool.begin().await?;
    let current = lock_item(&mut transaction, entity_type, id).await?;
    ensure_revision(&current, expected)?;
    let next_status = assignment_status(current.status, assigned_to);
    let sql = format!(
        "UPDATE {} SET assigned_to=$2,status=$3,revision=revision+1,updated_at=now() WHERE id=$1 RETURNING updated_at",
        table(entity_type),
    );
    sqlx::query(&sql)
        .bind(id)
        .bind(assigned_to)
        .bind(next_status.label())
        .execute(&mut *transaction)
        .await?;
    if next_status != current.status {
        insert_status_history(
            &mut transaction,
            entity_type,
            id,
            current.status,
            next_status,
            reason,
            actor,
        )
        .await?;
    }
    transaction.commit().await?;
    get(state, entity_type, id).await
}

pub async fn update_status(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
    expected: i64,
    next_status: BusinessInboxStatus,
    reason: &str,
    actor: &str,
) -> Result<BusinessInboxItem, ApiError> {
    validate_reason(reason)?;
    let pool = require_pool(state)?;
    let mut transaction = pool.begin().await?;
    let current = lock_item(&mut transaction, entity_type, id).await?;
    ensure_revision(&current, expected)?;
    if !transition_allowed(current.status, next_status) {
        return Err(
            ApiError::conflict("The requested business status transition is not allowed.")
                .with_code("business_status_transition_invalid"),
        );
    }
    let sql = format!(
        "UPDATE {} SET status=$2,revision=revision+1,updated_at=now() WHERE id=$1",
        table(entity_type),
    );
    sqlx::query(&sql)
        .bind(id)
        .bind(next_status.label())
        .execute(&mut *transaction)
        .await?;
    insert_status_history(
        &mut transaction,
        entity_type,
        id,
        current.status,
        next_status,
        reason,
        actor,
    )
    .await?;
    transaction.commit().await?;
    get(state, entity_type, id).await
}

pub async fn add_note(
    state: &AppState,
    entity_type: BusinessEntityType,
    id: Uuid,
    expected: i64,
    body: &str,
    reason: &str,
    created_by: Uuid,
) -> Result<BusinessInboxDetail, ApiError> {
    validate_reason(reason)?;
    if !(1..=4000).contains(&body.trim().chars().count()) {
        return Err(ApiError::validation(std::collections::BTreeMap::from([(
            "body".into(),
            vec!["Note body must contain 1 to 4000 characters.".into()],
        )])));
    }
    let pool = require_pool(state)?;
    let mut transaction = pool.begin().await?;
    let current = lock_item(&mut transaction, entity_type, id).await?;
    ensure_revision(&current, expected)?;
    sqlx::query(
        r#"INSERT INTO business_internal_notes
           (id,entity_type,entity_id,body,created_by,created_at)
           VALUES ($1,$2,$3,$4,$5,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(entity_type.label())
    .bind(id)
    .bind(body.trim())
    .bind(created_by)
    .execute(&mut *transaction)
    .await?;
    let sql = format!(
        "UPDATE {} SET revision=revision+1,updated_at=now() WHERE id=$1",
        table(entity_type),
    );
    sqlx::query(&sql)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    detail(state, entity_type, id).await
}
