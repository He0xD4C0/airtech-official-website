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

async fn start_sync_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StartSyncRequest>,
) -> Result<Response, ApiError> {
    let actor = actor(&headers);
    let idempotency = match begin_idempotency(
        &state,
        "admin.feishu.sync.start",
        &headers,
        &json!({"actor": &actor, "request": &request}),
    )
    .await?
    {
        IdempotencyOutcome::Replay(replay) => {
            let status = replay.status()?;
            let run: SyncRun = replay.decode()?;
            return Ok((status, Json(run)).into_response());
        }
        IdempotencyOutcome::Fresh(context) => context,
    };
    if request.mapping_version.trim().is_empty() {
        return Err(ApiError::bad_request("mappingVersion is required."));
    }
    let adapter_unavailable = state.pool.is_none();
    let now = Utc::now();
    let run = SyncRun {
        id: Uuid::new_v4(),
        source: "feishu".into(),
        dry_run: request.dry_run,
        mapping_version: request.mapping_version,
        status: if adapter_unavailable {
            SyncRunStatus::Failed
        } else {
            SyncRunStatus::Queued
        },
        resume_cursor: request.cursor,
        records_seen: 0,
        records_valid: 0,
        conflict_count: 0,
        started_at: now,
        completed_at: adapter_unavailable.then_some(now),
        error: adapter_unavailable.then(|| {
            "Feishu synchronization requires PostgreSQL and a configured provider adapter; no synchronization ran."
                .into()
        }),
    };
    state.enqueue_sync_run(&run).await?;
    if state.pool.is_none() {
        state
            .data
            .write()
            .await
            .sync_runs
            .insert(run.id, run.clone());
    }
    audit(
        &state,
        &headers,
        &actor,
        "feishu.sync.queue",
        "syncRun",
        Some(run.id),
        None,
        Some(json!(run)),
        Some("Queue Feishu staging synchronization".into()),
    )
    .await?;
    idempotency
        .complete(&state, &run, StatusCode::ACCEPTED)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(run)).into_response())
}

async fn list_conflicts(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::SyncConflict>>, ApiError> {
    let mut values: Vec<_> = if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT id,sync_run_id,product_id,source_record_id,field_diffs,resolved_at,resolution
               FROM sync_conflicts ORDER BY id"#,
        )
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
    Ok(Json(paginate_by_id(
        "admin.feishuConflicts",
        values,
        query,
        |entry| entry.id,
    )?))
}
