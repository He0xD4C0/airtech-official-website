async fn list_sync_runs(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<SyncRun>>, ApiError> {
    let values = state.list_sync_runs().await?;
    Ok(Json(paginate_by_id(
        "admin.feishuSyncRuns",
        values,
        query,
        |run| run.id,
    )?))
}

async fn get_feishu_connection_status(
    State(state): State<AppState>,
) -> Result<Json<crate::models::FeishuConnectionStatus>, ApiError> {
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("PostgreSQL is required for Feishu connection status.")
    })?;
    let connector = sqlx::query(
        r#"SELECT id,display_name,enabled,encrypted_configuration IS NOT NULL AS configured,updated_at
           FROM source_connectors WHERE connector_type='feishu' ORDER BY updated_at DESC LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;
    let latest_sync = state.list_sync_runs().await?.into_iter().next();
    Ok(Json(match connector {
        Some(row) => crate::models::FeishuConnectionStatus {
            connector_id: Some(row.try_get("id")?),
            display_name: Some(row.try_get("display_name")?),
            configured: row.try_get("configured")?,
            enabled: row.try_get("enabled")?,
            runnable: false,
            unavailable_reason: Some(
                "Feishu provider adapter is not connected; synchronization is disabled.".into(),
            ),
            updated_at: Some(row.try_get("updated_at")?),
            latest_sync,
        },
        None => crate::models::FeishuConnectionStatus {
            connector_id: None,
            display_name: None,
            configured: false,
            enabled: false,
            runnable: false,
            unavailable_reason: Some(
                "Feishu provider adapter is not connected; synchronization is disabled.".into(),
            ),
            updated_at: None,
            latest_sync,
        },
    }))
}

async fn list_feishu_mappings(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::SyncMapping>>, ApiError> {
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("PostgreSQL is required for Feishu mappings.")
    })?;
    let rows = sqlx::query(
        r#"SELECT id,connector_id,version,mapping,schema_version,active,created_at
           FROM sync_mappings ORDER BY active DESC,created_at DESC,id DESC"#,
    )
    .fetch_all(pool)
    .await?;
    let mappings = rows
        .into_iter()
        .map(|row| {
            Ok(crate::models::SyncMapping {
                id: row.try_get("id")?,
                connector_id: row.try_get("connector_id")?,
                version: row.try_get("version")?,
                mapping: row.try_get("mapping")?,
                schema_version: row.try_get("schema_version")?,
                active: row.try_get("active")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    Ok(Json(paginate_by_id(
        "admin.feishuMappings",
        mappings,
        query,
        |mapping| mapping.id,
    )?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StagingListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    sync_run_id: Option<Uuid>,
    status: Option<String>,
    q: Option<String>,
}

async fn list_feishu_staging(
    State(state): State<AppState>,
    Query(query): Query<StagingListQuery>,
) -> Result<Json<crate::models::StagingRecordPage>, ApiError> {
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("PostgreSQL is required for Feishu staging.")
    })?;
    let status = query
        .status
        .map(|value| {
            serde_json::from_value::<crate::models::StagingValidationStatus>(Value::String(value))
                .map(serialized_enum_label)
                .map_err(|_| ApiError::bad_request("status is not a staging validation state."))
        })
        .transpose()?;
    let search = parse_query_text(query.q)?;
    let rows = sqlx::query(
        r#"SELECT id,sync_run_id,source_snapshot_id,source_record_id,validation_status,
                  normalized_payload,validation_errors,created_at
           FROM staging_records
           WHERE ($1::uuid IS NULL OR sync_run_id=$1)
             AND ($2::text IS NULL OR validation_status=$2)
             AND ($3::text IS NULL OR source_record_id ILIKE '%' || $3 || '%')
           ORDER BY created_at DESC,id DESC"#,
    )
    .bind(query.sync_run_id)
    .bind(status)
    .bind(search)
    .fetch_all(pool)
    .await?;
    let values = rows
        .into_iter()
        .map(|row| {
            Ok(crate::models::StagingRecord {
                id: row.try_get("id")?,
                sync_run_id: row.try_get("sync_run_id")?,
                source_snapshot_id: row.try_get("source_snapshot_id")?,
                source_record_id: row.try_get("source_record_id")?,
                validation_status: serde_json::from_value(Value::String(
                    row.try_get("validation_status")?,
                ))
                .map_err(|_| ApiError::service_unavailable("Stored staging status is invalid."))?,
                normalized_payload: row.try_get("normalized_payload")?,
                validation_errors: serde_json::from_value(row.try_get("validation_errors")?)
                    .map_err(|_| ApiError::service_unavailable("Stored validation errors are invalid."))?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let total = values.len();
    let page = paginate_by_id(
        "admin.feishuStaging",
        values,
        CursorQuery {
            cursor: query.cursor,
            limit: query.limit,
        },
        |entry| entry.id,
    )?;
    Ok(Json(crate::models::StagingRecordPage {
        items: page.items,
        next_cursor: page.next_cursor,
        total,
    }))
}

async fn start_sync_run(
    State(_state): State<AppState>,
    _headers: HeaderMap,
    Json(_request): Json<StartSyncRequest>,
) -> Result<Response, ApiError> {
    Err(ApiError::conflict(
        "Feishu provider adapter is not connected; synchronization is disabled.",
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConflictListQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    q: Option<String>,
    open_only: Option<bool>,
}

async fn list_conflicts(
    State(state): State<AppState>,
    Query(query): Query<ConflictListQuery>,
) -> Result<Json<crate::models::SyncConflictPage>, ApiError> {
    let search = parse_query_text(query.q)?;
    let open_only = query.open_only.unwrap_or(true);
    let mut values: Vec<_> = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT id,sync_run_id,product_id,source_record_id,field_diffs,resolved_at,resolution
               FROM sync_conflicts
               WHERE (NOT $1 OR resolved_at IS NULL)
                 AND ($2::text IS NULL OR source_record_id ILIKE '%' || $2 || '%')
               ORDER BY resolved_at NULLS FIRST,id DESC"#,
        )
        .bind(open_only)
        .bind(search)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(crate::models::SyncConflict {
                    id: row.try_get("id")?,
                    sync_run_id: row.try_get("sync_run_id")?,
                    product_id: row.try_get("product_id")?,
                    source_record_id: row.try_get("source_record_id")?,
                    diffs: serde_json::from_value(row.try_get("field_diffs")?).map_err(|_| {
                        ApiError::service_unavailable("Stored sync conflict data is invalid.")
                    })?,
                    resolved_at: row.try_get("resolved_at")?,
                    resolution: row.try_get("resolution")?,
                    revision: if row.try_get::<Option<DateTime<Utc>>, _>("resolved_at")?.is_some() {
                        2
                    } else {
                        1
                    },
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        state
            .data
            .read()
            .await
            .conflicts
            .values()
            .cloned()
            .collect()
    };
    values.sort_by_key(|entry| entry.id);
    let total = values.len();
    let page = paginate_by_id(
        "admin.feishuConflicts",
        values,
        CursorQuery {
            cursor: query.cursor,
            limit: query.limit,
        },
        |entry| entry.id,
    )?;
    Ok(Json(crate::models::SyncConflictPage {
        items: page.items,
        next_cursor: page.next_cursor,
        total,
    }))
}

fn serialized_enum_label(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
