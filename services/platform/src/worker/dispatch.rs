/// Claim and execute at most one durable job. The long-running worker uses the
/// same entrypoint as integration tests and bounded operational invocations,
/// which keeps retry and operation-status behavior observable without a
/// second implementation.
pub async fn run_one_pending_job(state: &AppState) -> Result<(), ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("PostgreSQL is required for the worker."))?;
    claim_and_run(
        pool,
        state.config.guest_raw_retention_days,
        state.config.guest_aggregate_retention_months,
        state.config.approved_product_master.as_ref(),
    )
    .await
}

async fn claim_outbox_event(pool: &PgPool) -> Result<(), ApiError> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"SELECT id, topic, aggregate_id, payload
           FROM outbox_events
           WHERE (status='pending' AND available_at <= now())
              OR (status='processing' AND locked_at < now() - interval '5 minutes')
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1"#,
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(row) = row else {
        transaction.rollback().await?;
        return Ok(());
    };
    let id: Uuid = row.try_get("id")?;
    let topic: String = row.try_get("topic")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let payload: Value = row.try_get("payload")?;
    sqlx::query(
        "UPDATE outbox_events SET status='processing', attempts=attempts+1, locked_at=now() WHERE id=$1",
    )
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    // Public SSR, internal search, and sitemap discovery all query the
    // published PostgreSQL projections at request time. A recognized
    // publication event therefore completes once the atomic projection is
    // visible; there is no process-local cache to invalidate. Provider-specific
    // CDN/search adapters can extend this branch later without making current
    // events fail forever merely because no external provider is configured.
    if is_public_projection_topic(&topic) {
        sqlx::query(
            r#"UPDATE outbox_events
               SET status='completed', locked_at=NULL, processed_at=now(),
                   payload=payload || jsonb_build_object(
                       'delivery', jsonb_build_object(
                           'publishedProjection','ready',
                           'ssrCache','requestTimeDatabaseRead',
                           'search','publishedProjection',
                           'sitemap','publishedProjection',
                           'externalAdapter','notConfigured'
                       )
                   )
               WHERE id=$1"#,
        )
        .bind(id)
        .execute(pool)
        .await?;
        tracing::info!(%id, %topic, %aggregate_id, payload = %payload, "published projection event completed");
        return Ok(());
    }

    tracing::error!(%id, %topic, "unknown outbox topic");
    sqlx::query(
        r#"UPDATE outbox_events
           SET status=CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END,
               available_at=now() + (attempts * interval '30 seconds'), locked_at=NULL,
               processed_at=CASE WHEN attempts >= max_attempts THEN now() ELSE NULL END
           WHERE id=$1"#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

fn is_public_projection_topic(topic: &str) -> bool {
    matches!(
        topic,
        "public.content.published"
            | "public.product.published"
            | "public.news.published"
            | "public.generalInformation.published"
    )
}

async fn claim_and_run(
    pool: &PgPool,
    guest_raw_retention_days: i64,
    guest_aggregate_retention_months: i64,
    approved_product_master: Option<&crate::config::ApprovedProductMaster>,
) -> Result<(), ApiError> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"SELECT id, job_type, payload, attempts, max_attempts
           FROM jobs
           WHERE (status = 'queued' AND available_at <= now() AND attempts < max_attempts)
              OR (status = 'running' AND updated_at < now() - interval '5 minutes')
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1"#,
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(row) = row else {
        transaction.rollback().await?;
        return Ok(());
    };
    let id: Uuid = row.try_get("id")?;
    let job_type: String = row.try_get("job_type")?;
    let payload: Value = row.try_get("payload")?;
    let attempts: i32 = row.try_get::<i32, _>("attempts")? + 1;
    let max_attempts: i32 = row.try_get("max_attempts")?;
    sqlx::query(
        "UPDATE jobs SET status='running', attempts=attempts+1, updated_at=now() WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE operation_runs SET status='running', updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if job_type == "feishuSync" {
        sqlx::query(
            r#"UPDATE sync_runs
               SET status='fetching',
                   payload=(jsonb_set(payload, '{status}', '"fetching"', true) - 'error')
               WHERE id=$1"#,
        )
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;

    let result = execute_job(
        pool,
        &job_type,
        payload,
        guest_raw_retention_days,
        guest_aggregate_retention_months,
        approved_product_master,
    )
    .await;
    match result {
        Ok(output) => {
            let mut transaction = pool.begin().await?;
            sqlx::query(
                "UPDATE jobs SET status='completed', result=$2, updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(&output)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE operation_runs SET status='completed', result=$2, updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(&output)
            .execute(&mut *transaction)
            .await?;
            if job_type == "feishuSync" {
                sqlx::query(
                    r#"UPDATE sync_runs
                       SET status='completed', completed_at=now(),
                           payload=(jsonb_set(
                               jsonb_set(payload, '{status}', '"completed"', true),
                               '{completedAt}', to_jsonb(now()), true
                           ) - 'error')
                       WHERE id=$1"#,
                )
                .bind(id)
                .execute(&mut *transaction)
                .await?;
            }
            transaction.commit().await?;
        }
        Err(detail) => {
            let output = json!({"error": &detail});
            let retrying = attempts < max_attempts;
            let status = if retrying { "queued" } else { "failed" };
            let mut transaction = pool.begin().await?;
            sqlx::query(
                r#"UPDATE jobs SET status=$2, result=$3, last_error=$4,
                   available_at=CASE WHEN $2='queued' THEN now() + ($5 * interval '30 seconds') ELSE available_at END,
                   updated_at=now() WHERE id=$1"#,
            )
            .bind(id)
            .bind(status)
            .bind(&output)
            .bind(&detail)
            .bind(attempts)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE operation_runs SET status=$2, result=$3, updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(status)
            .bind(&output)
            .execute(&mut *transaction)
            .await?;
            if job_type == "feishuSync" {
                let statement = if retrying {
                    r#"UPDATE sync_runs
                       SET status='queued',
                           payload=jsonb_set(payload, '{status}', '"queued"', true)
                               || jsonb_build_object('error', $2::text)
                       WHERE id=$1"#
                } else {
                    r#"UPDATE sync_runs
                       SET status='failed', completed_at=now(),
                           payload=jsonb_set(
                               jsonb_set(payload, '{status}', '"failed"', true),
                               '{completedAt}', to_jsonb(now()), true
                           ) || jsonb_build_object('error', $2::text)
                       WHERE id=$1"#
                };
                sqlx::query(statement)
                    .bind(id)
                    .bind(&detail)
                    .execute(&mut *transaction)
                    .await?;
            }
            if job_type == "productImport" {
                sqlx::query(
                    r#"UPDATE product_import_runs
                       SET status=CASE WHEN $2 THEN 'readyToPublish' ELSE 'failed' END,
                           completed_at=CASE WHEN $2 THEN NULL ELSE now() END
                       WHERE id=$1 AND status <> 'completed'"#,
                )
                .bind(id)
                .bind(retrying)
                .execute(&mut *transaction)
                .await?;
            }
            transaction.commit().await?;
        }
    }
    Ok(())
}

async fn execute_job(
    pool: &PgPool,
    job_type: &str,
    payload: Value,
    guest_raw_retention_days: i64,
    guest_aggregate_retention_months: i64,
    approved_product_master: Option<&crate::config::ApprovedProductMaster>,
) -> Result<Value, String> {
    match job_type {
        "migrationPreflight" => {
            let version = sqlx::query_scalar::<_, String>("SHOW server_version")
                .fetch_one(pool)
                .await
                .map_err(|error| error.to_string())?;
            let flyway_status = crate::flyway::read_status(pool).await.map_err(|error| {
                format!(
                    "Flyway schema history is unavailable; run the deployment migration first: {error}"
                )
            })?;
            if !flyway_status.is_current() {
                return Err(format!(
                    "Flyway schema history is not current: version={:?}, failed={}, coveredRequiredVersions={}, legacyBaselinePresent={}",
                    flyway_status.current_version,
                    flyway_status.failed_migrations,
                    flyway_status.covered_required_versions,
                    flyway_status.legacy_baseline_present
                ));
            }
            Ok(json!({
                "checkedAt": Utc::now(),
                "migrationTool": "flyway",
                "currentVersion": flyway_status.current_version,
                "failedMigrations": flyway_status.failed_migrations,
                "coveredRequiredVersions": flyway_status.covered_required_versions,
                "legacyBaselinePresent": flyway_status.legacy_baseline_present,
                "serverVersion": version
            }))
        }
        "migrationApply" => Err(
            "Schema migrations are deployment-only and must run through Flyway; the application worker cannot apply them."
                .into(),
        ),
        "retentionApply" => {
            apply_retention_with_deployment_defaults(
                pool,
                guest_raw_retention_days,
                guest_aggregate_retention_months,
            )
            .await
        }
        "productImport" => {
            let import_run_id = product_import_id_from_job_payload(&payload)?;
            let report = promote_staged_product_import(
                pool,
                import_run_id,
                approved_product_master,
            )
                .await
                .map_err(|error| error.to_string())?;
            Ok(json!({"import": report}))
        }
        "searchReindex" | "cacheInvalidate" | "feishuSync" => Err(
            "The required provider adapter is not configured; the task was not completed."
                .into(),
        ),
        "backup" | "restoreValidate" => Err(
            "A configured encrypted backup/isolated-restore executor is required; no shell command was run."
                .into(),
        ),
        _ => Err("Unknown predefined job type; arbitrary commands are not executed by the worker.".into()),
    }
}

fn product_import_id_from_job_payload(payload: &Value) -> Result<Uuid, String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "Product import job payload must contain only importRunId.".to_owned())?;
    if object.len() != 1 {
        return Err("Product import job payload must contain only importRunId.".to_owned());
    }
    object
        .get("importRunId")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<Uuid>().ok())
        .ok_or_else(|| "Product import job importRunId is invalid.".to_owned())
}
