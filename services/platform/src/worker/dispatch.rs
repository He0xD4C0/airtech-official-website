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

/// Dispatch at most one durable outbox event through the same path used by the
/// long-running worker.
pub async fn run_one_pending_outbox_event(pool: &PgPool) -> Result<(), ApiError> {
    claim_outbox_event(pool).await
}

async fn claim_outbox_event(pool: &PgPool) -> Result<(), ApiError> {
    let mut transaction = pool.begin().await?;
    let row = sqlx::query(
        r#"SELECT id, topic, aggregate_id, payload, attempts, max_attempts
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

    let outbox_status = sqlx::query_scalar::<_, String>(
        r#"UPDATE outbox_events
           SET status=CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END,
               available_at=now() + (attempts * interval '30 seconds'), locked_at=NULL,
               processed_at=CASE WHEN attempts >= max_attempts THEN now() ELSE NULL END
           WHERE id=$1
           RETURNING status"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    tracing::error!(event = %id, %topic, %outbox_status, "unknown outbox topic");
    Ok(())
}

fn is_public_projection_topic(topic: &str) -> bool {
    matches!(
        topic,
        "public.content.published"
            | "public.content.unpublished"
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
    if terminalize_one_expired_exhausted_job(&mut transaction).await? {
        transaction.commit().await?;
        return Ok(());
    }
    let lease_owner = format!("worker:{}", Uuid::new_v4());
    let row = sqlx::query(
        r#"SELECT id, job_type, payload, attempts, max_attempts
           FROM jobs
           WHERE (status = 'queued' AND available_at <= now() AND attempts < max_attempts)
              OR (status = 'running' AND attempts < max_attempts AND (
                    lease_expires_at < now()
                    OR (lease_expires_at IS NULL AND updated_at < now() - interval '5 minutes')
                 ))
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
    let lease = ClaimedJobLease {
        job_id: id,
        owner: lease_owner,
    };
    let job_type: String = row.try_get("job_type")?;
    let payload: Value = row.try_get("payload")?;
    let attempts: i32 = row.try_get::<i32, _>("attempts")? + 1;
    let max_attempts: i32 = row.try_get("max_attempts")?;
    sqlx::query(
        r#"UPDATE jobs SET status='running', attempts=attempts+1,
           lease_owner=$2,
           lease_expires_at=now() + ($3 * interval '1 second'),updated_at=now()
           WHERE id=$1"#,
    )
    .bind(id)
    .bind(&lease.owner)
    .bind(JOB_LEASE_SECONDS)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE operation_runs SET status='running', updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    let result = execute_with_job_lease(
        pool,
        &lease,
        execute_job(
            pool,
            &job_type,
            payload,
            guest_raw_retention_days,
            guest_aggregate_retention_months,
            approved_product_master,
        ),
    )
    .await?;
    let LeasedExecution::Finished(result) = result else {
        tracing::warn!(job_id = %id, job_type = %job_type, "job execution stopped after its lease was lost");
        return Ok(());
    };
    match result {
        Ok(output) => {
            if !persist_job_success(pool, &lease, &output).await? {
                tracing::warn!(job_id = %id, job_type = %job_type, "discarded completed job result after lease ownership changed");
            }
        }
        Err(detail) => {
            if !persist_job_failure(
                pool,
                &lease,
                &job_type,
                attempts,
                max_attempts,
                &detail,
            )
            .await?
            {
                tracing::warn!(job_id = %id, job_type = %job_type, "discarded failed job result after lease ownership changed");
            }
        }
    }
    Ok(())
}

const EXPIRED_ATTEMPT_BUDGET_ERROR: &str =
    "job_lease_expired: the prior worker exhausted the attempt budget without a durable result";

/// Finalize one abandoned last attempt before claiming new work. The row lock
/// and old-owner predicate make the job, operation, and related state one
/// owner-fenced transaction. A worker that eventually returns with the stale
/// owner can no longer overwrite this terminal result.
async fn terminalize_one_expired_exhausted_job(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<bool, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,job_type,payload,lease_owner
           FROM jobs
           WHERE status='running' AND attempts >= max_attempts
             AND (
               lease_expires_at < now()
               OR (lease_expires_at IS NULL AND updated_at < now() - interval '5 minutes')
             )
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1"#,
    )
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(row) = row else {
        return Ok(false);
    };

    let id: Uuid = row.try_get("id")?;
    let job_type: String = row.try_get("job_type")?;
    let prior_owner: Option<String> = row.try_get("lease_owner")?;
    let output = json!({"error": EXPIRED_ATTEMPT_BUDGET_ERROR});
    let updated = sqlx::query(
        r#"UPDATE jobs
           SET status='failed',result=$2,last_error=$3,
               lease_owner=NULL,lease_expires_at=NULL,
               updated_at=now()
           WHERE id=$1 AND status='running'
             AND lease_owner IS NOT DISTINCT FROM $4
             AND attempts >= max_attempts
             AND (
               lease_expires_at < now()
               OR (lease_expires_at IS NULL AND updated_at < now() - interval '5 minutes')
             )"#,
    )
    .bind(id)
    .bind(&output)
    .bind(EXPIRED_ATTEMPT_BUDGET_ERROR)
    .bind(prior_owner.as_deref())
    .execute(&mut **transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::internal(
            "The exhausted job changed while its terminal state was being persisted.",
        ));
    }

    sqlx::query("UPDATE operation_runs SET status='failed',result=$2,updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(&output)
        .execute(&mut **transaction)
        .await?;
    persist_related_failure(
        transaction,
        id,
        &job_type,
        false,
    )
    .await?;
    tracing::error!(
        job_id = %id,
        job_type = %job_type,
        prior_owner = ?prior_owner,
        "expired job attempt budget exhausted; job finalized without re-execution"
    );
    Ok(true)
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
        _ => Err("This retired job type is no longer executable.".into()),
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
