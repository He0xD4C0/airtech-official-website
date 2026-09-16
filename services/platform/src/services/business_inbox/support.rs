use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        BusinessContact, BusinessEntityType, BusinessInboxItem, BusinessInboxStatus,
        BusinessInternalNote, BusinessPii, BusinessStatusHistoryEntry, ContactRequest,
        RfqSubmission,
    },
    state::AppState,
};

pub(super) fn item_from_row(
    entity_type: BusinessEntityType,
    row: sqlx::postgres::PgRow,
) -> Result<BusinessInboxItem, ApiError> {
    let payload: Value = row.try_get("payload")?;
    let (journey, topic, contact, product_context, consent) = match entity_type {
        BusinessEntityType::Rfq => {
            let request = decode_rfq(payload)?.request;
            (
                Some(request.journey),
                None,
                request.contact,
                request.product_context,
                request.consent,
            )
        }
        BusinessEntityType::Contact => {
            let request = decode_contact(payload)?.request;
            (
                None,
                Some(request.topic),
                request.contact,
                None,
                request.consent,
            )
        }
    };
    Ok(BusinessInboxItem {
        id: row.try_get("id")?,
        entity_type,
        reference: row.try_get("reference")?,
        journey,
        topic,
        organization: contact.company,
        country_or_region: contact.country_or_region,
        product_context,
        source_path: row.try_get("source_path")?,
        locale: row.try_get("locale")?,
        consent,
        status: parse_status(row.try_get("status")?)?,
        revision: row.try_get("revision")?,
        assigned_to: row.try_get("assigned_to")?,
        submitted_at: row.try_get("submitted_at")?,
        updated_at: row.try_get("updated_at")?,
        retention_until: row.try_get("retention_until")?,
    })
}

pub(super) async fn lock_item(
    transaction: &mut Transaction<'_, Postgres>,
    entity_type: BusinessEntityType,
    id: Uuid,
) -> Result<BusinessInboxItem, ApiError> {
    let sql = format!(
        r#"SELECT id,reference,status,revision,assigned_to,submitted_at,updated_at,
                  retention_until,source_path,locale,payload FROM {} WHERE id=$1 FOR UPDATE"#,
        table(entity_type),
    );
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| ApiError::not_found("Business inbox item was not found."))?;
    item_from_row(entity_type, row)
}

pub(super) async fn insert_status_history(
    transaction: &mut Transaction<'_, Postgres>,
    entity_type: BusinessEntityType,
    id: Uuid,
    before: BusinessInboxStatus,
    after: BusinessInboxStatus,
    reason: &str,
    actor: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO business_status_history
           (id,entity_type,entity_id,from_status,to_status,note,changed_by,changed_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(entity_type.label())
    .bind(id)
    .bind(before.label())
    .bind(after.label())
    .bind(reason.trim())
    .bind(actor)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(super) fn note_from_row(row: sqlx::postgres::PgRow) -> Result<BusinessInternalNote, ApiError> {
    Ok(BusinessInternalNote {
        id: row.try_get("id")?,
        entity_type: parse_entity_type(row.try_get("entity_type")?)?,
        entity_id: row.try_get("entity_id")?,
        body: row.try_get("body")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
    })
}

pub(super) fn status_history_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<BusinessStatusHistoryEntry, ApiError> {
    let from_status = row
        .try_get::<Option<String>, _>("from_status")?
        .map(parse_status)
        .transpose()?;
    Ok(BusinessStatusHistoryEntry {
        id: row.try_get("id")?,
        from_status,
        to_status: parse_status(row.try_get("to_status")?)?,
        reason: row.try_get("note")?,
        changed_by: row.try_get("changed_by")?,
        changed_at: row.try_get("changed_at")?,
    })
}

pub(super) fn pii_from_contact(contact: BusinessContact, message: Option<String>) -> BusinessPii {
    BusinessPii {
        name: contact.name,
        email: contact.email,
        phone: contact.phone,
        company: contact.company,
        country_or_region: contact.country_or_region,
        message,
    }
}

pub(super) fn decode_rfq(payload: Value) -> Result<RfqSubmission, ApiError> {
    serde_json::from_value(payload)
        .map_err(|_| ApiError::service_unavailable("Stored RFQ payload is invalid."))
}

pub(super) fn decode_contact(payload: Value) -> Result<ContactRequest, ApiError> {
    serde_json::from_value(payload)
        .map_err(|_| ApiError::service_unavailable("Stored contact payload is invalid."))
}

fn parse_status(value: String) -> Result<BusinessInboxStatus, ApiError> {
    serde_json::from_value(Value::String(value))
        .map_err(|_| ApiError::service_unavailable("Stored business status is invalid."))
}

fn parse_entity_type(value: String) -> Result<BusinessEntityType, ApiError> {
    serde_json::from_value(Value::String(value))
        .map_err(|_| ApiError::service_unavailable("Stored business entity type is invalid."))
}

pub(super) fn table(entity_type: BusinessEntityType) -> &'static str {
    match entity_type {
        BusinessEntityType::Rfq => "rfq_submissions",
        BusinessEntityType::Contact => "contact_requests",
    }
}

pub(super) fn require_pool(state: &AppState) -> Result<&PgPool, ApiError> {
    Ok(&state.pool)
}

pub(super) fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if (3..=2000).contains(&reason.trim().chars().count()) {
        Ok(())
    } else {
        Err(ApiError::validation(std::collections::BTreeMap::from([(
            "reason".into(),
            vec!["Reason must contain 3 to 2000 characters.".into()],
        )])))
    }
}

pub(super) fn ensure_revision(item: &BusinessInboxItem, expected: i64) -> Result<(), ApiError> {
    if item.revision == expected {
        Ok(())
    } else {
        Err(
            ApiError::conflict("The inbox item changed; reload before mutating it.")
                .with_code("business_revision_conflict"),
        )
    }
}

pub(super) fn assignment_status(
    current: BusinessInboxStatus,
    assignee: Option<Uuid>,
) -> BusinessInboxStatus {
    match (current, assignee) {
        (BusinessInboxStatus::New | BusinessInboxStatus::Triaged, Some(_)) => {
            BusinessInboxStatus::Assigned
        }
        (BusinessInboxStatus::Assigned, None) => BusinessInboxStatus::Triaged,
        _ => current,
    }
}

pub(super) fn transition_allowed(before: BusinessInboxStatus, after: BusinessInboxStatus) -> bool {
    use BusinessInboxStatus::*;
    before == after
        || matches!(
            (before, after),
            (New, Triaged | Assigned | Spam)
                | (Triaged, Assigned | Qualified | Closed | Spam)
                | (Assigned, Triaged | Qualified | Closed | Spam)
                | (Qualified, Assigned | Closed | Spam)
                | (Closed, Triaged)
                | (Spam, Triaged)
        )
}
