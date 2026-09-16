use axum::body::Bytes;
use chrono::{DateTime, Utc};
use futures_util::{stream, stream::BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{AuditEvent, AuditEventPage},
    pagination::{
        cursor_limit, decode_scoped_cursor_compat, encode_scoped_cursor, CursorQuery, DecodedCursor,
    },
    services::request_metrics::LegacyCursorEndpoint,
    state::AppState,
};

const CSV_HEADER: &str =
    "id,occurred_at,actor,action,resource_type,resource_id,current_version,request_id,reason\r\n";
const CSV_CHUNK_SIZE: i64 = 500;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub actor: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub q: Option<String>,
}

#[derive(Clone, Debug)]
struct AuditFilter {
    actor: Option<String>,
    action: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<Uuid>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    q: Option<String>,
    scope: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AuditCursor {
    occurred_at: DateTime<Utc>,
    id: Uuid,
}

struct CsvState {
    pool: PgPool,
    filter: AuditFilter,
    cursor: Option<AuditCursor>,
    finished: bool,
}

pub type AuditCsvStream = BoxStream<'static, Result<Bytes, sqlx::Error>>;

pub async fn list(state: &AppState, query: AuditQuery) -> Result<AuditEventPage, ApiError> {
    let pool = &state.pool;
    let filter = AuditFilter::from_query(&query)?;
    let pagination = CursorQuery {
        cursor: query.cursor,
        limit: query.limit,
    };
    let limit = cursor_limit(&pagination)?;
    let cursor = match pagination.cursor.as_deref() {
        Some(value) => Some(resolve_cursor(state, &filter, value).await?),
        None => None,
    };

    let mut count = QueryBuilder::<Postgres>::new("SELECT count(*) FROM audit_log WHERE true");
    push_filters(&mut count, &filter);
    let total: i64 = count.build_query_scalar().fetch_one(pool).await?;

    let rows = fetch_rows(pool, &filter, cursor.as_ref(), limit as i64 + 1, true).await?;
    let has_more = rows.len() > limit;
    let mut items = rows
        .into_iter()
        .take(limit)
        .map(decode_event)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        items
            .last()
            .map(|event| {
                encode_scoped_cursor(
                    &filter.scope,
                    &AuditCursor {
                        occurred_at: event.occurred_at,
                        id: event.id,
                    },
                )
            })
            .transpose()?
    } else {
        None
    };
    items.shrink_to_fit();
    Ok(AuditEventPage {
        items,
        next_cursor,
        total: usize::try_from(total).unwrap_or(usize::MAX),
    })
}

async fn resolve_cursor(
    state: &AppState,
    filter: &AuditFilter,
    value: &str,
) -> Result<AuditCursor, ApiError> {
    match decode_scoped_cursor_compat::<Uuid, AuditCursor>(&filter.scope, value)? {
        DecodedCursor::Current(cursor) => Ok(cursor),
        DecodedCursor::Legacy(id) => {
            let mut builder =
                QueryBuilder::<Postgres>::new("SELECT occurred_at FROM audit_log WHERE id=");
            builder.push_bind(id);
            push_filters(&mut builder, filter);
            let occurred_at = builder
                .build_query_scalar::<DateTime<Utc>>()
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::AdminAudit);
            Ok(AuditCursor { occurred_at, id })
        }
    }
}

pub fn csv_stream(pool: PgPool, query: AuditQuery) -> Result<AuditCsvStream, ApiError> {
    let filter = AuditFilter::from_query(&query)?;
    let header = stream::once(async { Ok(Bytes::from_static(CSV_HEADER.as_bytes())) });
    let rows = stream::try_unfold(
        CsvState {
            pool,
            filter,
            cursor: None,
            finished: false,
        },
        |mut state| async move {
            if state.finished {
                return Ok(None);
            }
            let rows = fetch_rows(
                &state.pool,
                &state.filter,
                state.cursor.as_ref(),
                CSV_CHUNK_SIZE,
                false,
            )
            .await?;
            if rows.is_empty() {
                return Ok(None);
            }
            let mut csv = String::new();
            let row_count = rows.len();
            for row in rows {
                let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
                let id: Uuid = row.try_get("id")?;
                state.cursor = Some(AuditCursor { occurred_at, id });
                let fields = [
                    id.to_string(),
                    occurred_at.to_rfc3339(),
                    row.try_get("actor")?,
                    row.try_get("action")?,
                    row.try_get("entity_type")?,
                    row.try_get::<Option<Uuid>, _>("entity_id")?
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    row.try_get::<Option<i64>, _>("current_version")?
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    row.try_get::<Uuid, _>("request_id")?.to_string(),
                    row.try_get::<Option<String>, _>("reason")?
                        .unwrap_or_default(),
                ];
                csv.push_str(&fields.map(|field| csv_cell(&field)).join(","));
                csv.push_str("\r\n");
            }
            state.finished = row_count < CSV_CHUNK_SIZE as usize;
            Ok(Some((Bytes::from(csv), state)))
        },
    );
    Ok(header.chain(rows).boxed())
}

impl AuditFilter {
    fn from_query(query: &AuditQuery) -> Result<Self, ApiError> {
        if query
            .from
            .zip(query.to)
            .is_some_and(|(from, to)| from >= to)
        {
            return Err(ApiError::bad_request("from must be earlier than to."));
        }
        let trim = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let q = trim(&query.q);
        if q.as_ref().is_some_and(|value| value.chars().count() > 200) {
            return Err(ApiError::bad_request("q must be at most 200 characters."));
        }
        let actor = trim(&query.actor);
        let action = trim(&query.action);
        let resource_type = trim(&query.resource_type);
        let scope = format!(
            "admin.audit|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            actor, action, resource_type, query.resource_id, query.from, query.to, q
        );
        Ok(Self {
            actor,
            action,
            resource_type,
            resource_id: query.resource_id,
            from: query.from,
            to: query.to,
            q,
            scope,
        })
    }
}

fn push_filters(builder: &mut QueryBuilder<'static, Postgres>, filter: &AuditFilter) {
    if let Some(actor) = &filter.actor {
        builder
            .push(" AND lower(actor)=lower(")
            .push_bind(actor.clone())
            .push(")");
    }
    if let Some(action) = &filter.action {
        builder.push(" AND action=").push_bind(action.clone());
    }
    if let Some(resource_type) = &filter.resource_type {
        builder
            .push(" AND entity_type=")
            .push_bind(resource_type.clone());
    }
    if let Some(resource_id) = filter.resource_id {
        builder.push(" AND entity_id=").push_bind(resource_id);
    }
    if let Some(from) = filter.from {
        builder.push(" AND occurred_at>=").push_bind(from);
    }
    if let Some(to) = filter.to {
        builder.push(" AND occurred_at<").push_bind(to);
    }
    if let Some(q) = &filter.q {
        builder
            .push(" AND to_tsvector('simple',coalesce(actor,'')||' '||coalesce(action,'')||' '||coalesce(entity_type,'')||' '||coalesce(reason,'')) @@ plainto_tsquery('simple',")
            .push_bind(q.clone())
            .push(")");
    }
}

async fn fetch_rows(
    pool: &PgPool,
    filter: &AuditFilter,
    cursor: Option<&AuditCursor>,
    limit: i64,
    include_documents: bool,
) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
    let columns = if include_documents {
        "id,actor,action,entity_type,entity_id,before_value,after_value,reason,current_version,request_id,occurred_at"
    } else {
        "id,actor,action,entity_type,entity_id,reason,current_version,request_id,occurred_at"
    };
    let mut builder =
        QueryBuilder::<Postgres>::new(format!("SELECT {columns} FROM audit_log WHERE true"));
    push_filters(&mut builder, filter);
    if let Some(cursor) = cursor {
        builder
            .push(" AND (occurred_at,id)<(")
            .push_bind(cursor.occurred_at)
            .push(",")
            .push_bind(cursor.id)
            .push(")");
    }
    builder
        .push(" ORDER BY occurred_at DESC,id DESC LIMIT ")
        .push_bind(limit);
    builder.build().fetch_all(pool).await
}

fn decode_event(row: sqlx::postgres::PgRow) -> Result<AuditEvent, sqlx::Error> {
    Ok(AuditEvent {
        id: row.try_get("id")?,
        actor: row.try_get("actor")?,
        action: row.try_get("action")?,
        entity_type: row.try_get("entity_type")?,
        entity_id: row.try_get("entity_id")?,
        before: row.try_get("before_value")?,
        after: row.try_get("after_value")?,
        reason: row.try_get("reason")?,
        current_version: row.try_get("current_version")?,
        request_id: row.try_get("request_id")?,
        occurred_at: row.try_get("occurred_at")?,
    })
}

fn csv_cell(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\r' | '\n'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
