use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        FieldDiff, ResolveSyncConflictRequest, SyncConflict, SyncConflictDecision, SyncConflictPage,
    },
    pagination::{
        cursor_limit, decode_scoped_cursor_compat, encode_scoped_cursor, CursorQuery, DecodedCursor,
    },
    services::request_metrics::LegacyCursorEndpoint,
    state::AppState,
};

#[derive(Deserialize, Serialize)]
struct ConflictCursor {
    id: Uuid,
}

pub struct ConflictFilter {
    pub search: Option<String>,
    pub open_only: bool,
}

pub async fn list_conflicts(
    state: &AppState,
    filter: ConflictFilter,
    query: CursorQuery,
) -> Result<SyncConflictPage, ApiError> {
    let scope = format!(
        "admin.feishuConflicts|{:?}|{}",
        filter.search, filter.open_only
    );
    let limit = cursor_limit(&query)?;
    let after = resolve_cursor(state, &filter, &scope, query.cursor.as_deref()).await?;
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM sync_conflicts WHERE (NOT $1 OR resolved_at IS NULL)
           AND ($2::text IS NULL OR source_record_id ILIKE '%' || $2 || '%')"#,
    )
    .bind(filter.open_only)
    .bind(filter.search.as_deref())
    .fetch_one(&state.pool)
    .await?;
    let rows = sqlx::query(
        r#"SELECT id,sync_run_id,product_id,source_record_id,field_diffs,resolved_at,resolution
           FROM sync_conflicts WHERE (NOT $1 OR resolved_at IS NULL)
             AND ($2::text IS NULL OR source_record_id ILIKE '%' || $2 || '%')
             AND ($3::uuid IS NULL OR id<$3) ORDER BY id DESC LIMIT $4"#,
    )
    .bind(filter.open_only)
    .bind(filter.search.as_deref())
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut items = rows
        .into_iter()
        .map(decode_conflict)
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = has_more
        .then(|| items.last())
        .flatten()
        .map(|item| encode_scoped_cursor(&scope, &ConflictCursor { id: item.id }))
        .transpose()?;
    Ok(SyncConflictPage {
        items,
        next_cursor,
        total: usize::try_from(total).unwrap_or(usize::MAX),
    })
}

async fn resolve_cursor(
    state: &AppState,
    filter: &ConflictFilter,
    scope: &str,
    value: Option<&str>,
) -> Result<Option<ConflictCursor>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    match decode_scoped_cursor_compat::<Uuid, ConflictCursor>(scope, value)? {
        DecodedCursor::Current(cursor) => Ok(Some(cursor)),
        DecodedCursor::Legacy(id) => {
            let exists = sqlx::query_scalar::<_, bool>(
                r#"SELECT EXISTS(SELECT 1 FROM sync_conflicts WHERE id=$1
                   AND (NOT $2 OR resolved_at IS NULL)
                   AND ($3::text IS NULL OR source_record_id ILIKE '%' || $3 || '%'))"#,
            )
            .bind(id)
            .bind(filter.open_only)
            .bind(filter.search.as_deref())
            .fetch_one(&state.pool)
            .await?;
            if !exists {
                return Err(ApiError::bad_request("cursor is invalid or has expired."));
            }
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::FeishuConflicts);
            Ok(Some(ConflictCursor { id }))
        }
    }
}

pub async fn resolve_conflict_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    request: &ResolveSyncConflictRequest,
    actor: &str,
) -> Result<SyncConflict, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,sync_run_id,product_id,source_record_id,field_diffs,resolved_at,resolution
           FROM sync_conflicts WHERE id=$1 FOR UPDATE"#,
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Sync conflict was not found."))?;
    if row
        .try_get::<Option<DateTime<Utc>>, _>("resolved_at")?
        .is_some()
    {
        return Err(ApiError::conflict("The sync conflict is already resolved.")
            .with_code("sync_conflict_resolved"));
    }
    let product_id = row.try_get("product_id")?;
    let diffs: Vec<FieldDiff> = serde_json::from_value(row.try_get("field_diffs")?)
        .map_err(|_| ApiError::service_unavailable("Stored sync conflict data is invalid."))?;
    if request.decision == SyncConflictDecision::KeepVerifiedLocal {
        insert_verified_local_overrides(transaction, product_id, &diffs, request, actor).await?;
    }
    let resolved_at = Utc::now();
    sqlx::query(
        "UPDATE sync_conflicts SET resolved_at=$2,resolution=$3,resolved_by=$4 WHERE id=$1 AND resolved_at IS NULL",
    )
    .bind(id)
    .bind(resolved_at)
    .bind(request.decision.label())
    .bind(actor)
    .execute(&mut **transaction)
    .await?;
    let sync_run_id = row.try_get("sync_run_id")?;
    sqlx::query(
        r#"UPDATE sync_runs SET
             conflict_count=(SELECT count(*) FROM sync_conflicts WHERE sync_run_id=$1 AND resolved_at IS NULL),
             status=CASE WHEN status='awaitingResolution' AND NOT EXISTS(
               SELECT 1 FROM sync_conflicts WHERE sync_run_id=$1 AND resolved_at IS NULL
             ) THEN 'readyToPublish' ELSE status END WHERE id=$1"#,
    )
    .bind(sync_run_id)
    .execute(&mut **transaction)
    .await?;
    Ok(SyncConflict {
        id,
        sync_run_id,
        product_id,
        source_record_id: row.try_get("source_record_id")?,
        diffs,
        resolved_at: Some(resolved_at),
        resolution: Some(request.decision.label().into()),
        revision: 2,
    })
}

fn decode_conflict(row: sqlx::postgres::PgRow) -> Result<SyncConflict, ApiError> {
    let resolved_at = row.try_get("resolved_at")?;
    Ok(SyncConflict {
        id: row.try_get("id")?,
        sync_run_id: row.try_get("sync_run_id")?,
        product_id: row.try_get("product_id")?,
        source_record_id: row.try_get("source_record_id")?,
        diffs: serde_json::from_value(row.try_get("field_diffs")?)
            .map_err(|_| ApiError::service_unavailable("Stored sync conflict data is invalid."))?,
        resolved_at,
        resolution: row.try_get("resolution")?,
        revision: if resolved_at.is_some() { 2 } else { 1 },
    })
}

async fn insert_verified_local_overrides(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    product_id: Option<Uuid>,
    diffs: &[FieldDiff],
    request: &ResolveSyncConflictRequest,
    actor: &str,
) -> Result<(), ApiError> {
    let product_id = product_id.ok_or_else(|| {
        ApiError::validation(BTreeMap::from([(
            "productId".into(),
            vec!["A bound Product is required to keep verified local values.".into()],
        )]))
    })?;
    let evidence = request
        .evidence_reference
        .as_deref()
        .unwrap_or_default()
        .trim();
    let expiry = request.expires_at.expect("validated expiry");
    let source_owned = diffs
        .iter()
        .filter(|diff| diff.source_owned)
        .collect::<Vec<_>>();
    if source_owned.is_empty() {
        return Err(ApiError::validation(BTreeMap::from([(
            "diffs".into(),
            vec!["No source-owned field can receive a temporary override.".into()],
        )])));
    }
    for diff in source_owned {
        let value = diff.local_value.clone().ok_or_else(|| {
            ApiError::validation(BTreeMap::from([(
                diff.field_path.clone(),
                vec!["The verified local value is missing; no value will be inferred.".into()],
            )]))
        })?;
        sqlx::query(
            r#"INSERT INTO product_temporary_overrides
               (id,product_id,field_path,value,reason,created_at,expires_at,resolved_by)
               VALUES ($1,$2,$3,$4,$5,now(),$6,$7)"#,
        )
        .bind(Uuid::new_v4())
        .bind(product_id)
        .bind(&diff.field_path)
        .bind(value)
        .bind(format!("{} Evidence: {}", request.reason.trim(), evidence))
        .bind(expiry)
        .bind(actor)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}
