async fn diagnose(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let row = sqlx::query(
        "SELECT current_database() AS database_name, current_user AS database_user, current_setting('server_version') AS server_version, to_regclass('public.jobs') IS NOT NULL AS schema_ready, to_regclass('public.flyway_schema_history') IS NOT NULL AS flyway_history_ready",
    )
    .fetch_one(pool)
    .await?;
    let schema_ready: bool = row.try_get("schema_ready")?;
    let flyway_history_ready: bool = row.try_get("flyway_history_ready")?;
    let flyway_status = if flyway_history_ready {
        Some(flyway::read_status(pool).await?)
    } else {
        None
    };
    let (queued_jobs, failed_jobs, pending_outbox): (i64, i64, i64) = if schema_ready {
        (
            sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status='queued'")
                .fetch_one(pool)
                .await?,
            sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status='failed'")
                .fetch_one(pool)
                .await?,
            sqlx::query_scalar(
                "SELECT count(*) FROM outbox_events WHERE status IN ('pending','processing')",
            )
            .fetch_one(pool)
            .await?,
        )
    } else {
        (0, 0, 0)
    };
    print_json(json!({
        "status": if schema_ready && flyway_status.as_ref().is_some_and(flyway::FlywayStatus::is_current) { "ok" } else { "migrationRequired" },
        "database": {
            "name": row.try_get::<String, _>("database_name")?,
            "user": row.try_get::<String, _>("database_user")?,
            "serverVersion": row.try_get::<String, _>("server_version")?,
            "schemaReady": schema_ready,
        },
        "migration": {
            "tool": "flyway",
            "historyReady": flyway_history_ready,
            "currentVersion": flyway_status.as_ref().and_then(|status| status.current_version),
            "failedMigrations": flyway_status.as_ref().map(|status| status.failed_migrations),
            "coveredRequiredVersions": flyway_status.as_ref().map(|status| status.covered_required_versions),
            "legacyBaselinePresent": flyway_status.as_ref().map(|status| status.legacy_baseline_present),
        },
        "workerBacklog": {
            "queuedJobs": queued_jobs,
            "failedJobs": failed_jobs,
            "pendingOutbox": pending_outbox,
        },
        "osUid": std::process::Command::new("id").arg("-u").output().ok().map(|value| String::from_utf8_lossy(&value.stdout).trim().to_owned()),
    }))?;
    Ok(())
}

async fn queue_feishu_sync(
    pool: &PgPool,
    dry_run: bool,
    mapping_version: String,
    cursor: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = Uuid::new_v4();
    let started_at = Utc::now();
    let run = SyncRun {
        id,
        source: "feishu".into(),
        dry_run,
        mapping_version,
        status: SyncRunStatus::Queued,
        resume_cursor: cursor,
        records_seen: 0,
        records_valid: 0,
        conflict_count: 0,
        started_at,
        completed_at: None,
        error: None,
    };
    let run_payload = serde_json::to_value(&run)?;
    let job_payload = json!({
        "source": "airtekctl",
        "syncRunId": id,
        "dryRun": dry_run,
        "mappingVersion": &run.mapping_version,
        "cursor": &run.resume_cursor,
    });
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id, source, dry_run, mapping_version, status, resume_cursor, records_seen,
            records_valid, conflict_count, started_at, completed_at, payload)
           VALUES ($1,'feishu',$2,$3,'queued',$4,0,0,0,$5,NULL,$6)"#,
    )
    .bind(id)
    .bind(dry_run)
    .bind(&run.mapping_version)
    .bind(&run.resume_cursor)
    .bind(started_at)
    .bind(run_payload)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO jobs (id, job_type, status, payload, available_at, created_at, updated_at) VALUES ($1,'feishuSync','queued',$2,$3,$3,$3)",
    )
    .bind(id)
    .bind(job_payload)
    .bind(started_at)
    .execute(&mut *transaction)
    .await?;
    insert_cli_audit(
        &mut transaction,
        "devtools.cli.feishuSync.queued",
        "syncRun",
        Some(id),
        json!({"dryRun": dry_run, "mappingVersion": &run.mapping_version}),
        "Development Feishu staging synchronization queued; no product was published.",
    )
    .await?;
    transaction.commit().await?;
    print_json(json!({
        "status": "queued",
        "jobId": id,
        "syncRunId": id,
        "jobType": "feishuSync",
        "dryRun": dry_run,
        "note": "Queued in Feishu staging; this does not publish products. The worker will fail explicitly when no Feishu provider adapter is configured.",
    }))?;
    Ok(())
}

async fn queue_operation(
    pool: &PgPool,
    job_type: &'static str,
    reason: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO operation_runs (id, kind, status, reason, created_at, updated_at) VALUES ($1,$2,'queued',$3,$4,$4)",
    )
    .bind(id)
    .bind(job_type)
    .bind(reason)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO jobs (id, job_type, status, payload, available_at, created_at, updated_at) VALUES ($1,$2,'queued',$3,$4,$4,$4)",
    )
    .bind(id)
    .bind(job_type)
    .bind(json!({"source": "airtekctl"}))
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    insert_cli_audit(
        &mut transaction,
        "devtools.cli.operation.queued",
        "backgroundOperation",
        Some(id),
        json!({"jobType": job_type}),
        reason,
    )
    .await?;
    transaction.commit().await?;
    print_json(json!({
        "status": "queued",
        "jobId": id,
        "operationId": id,
        "jobType": job_type,
        "note": "Queued is not completed; the worker may fail the job when its required provider adapter is not configured.",
    }))?;
    Ok(())
}
