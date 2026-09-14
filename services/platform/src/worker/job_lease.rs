const JOB_LEASE_SECONDS: i64 = 5 * 60;
const JOB_LEASE_RENEW_INTERVAL: Duration = Duration::from_secs(60);
const RENEW_JOB_LEASE_SQL: &str = r#"UPDATE jobs
   SET lease_expires_at=now() + ($3 * interval '1 second'),updated_at=now()
   WHERE id=$1 AND status='running' AND lease_owner=$2"#;
const COMPLETE_JOB_SQL: &str = r#"UPDATE jobs SET status='completed',result=$2,last_error=NULL,
   lease_owner=NULL,lease_expires_at=NULL,updated_at=now()
   WHERE id=$1 AND status='running' AND lease_owner=$3"#;
const FAIL_JOB_SQL: &str = r#"UPDATE jobs SET status=$2,result=$3,last_error=$4,
   available_at=CASE WHEN $2='queued' THEN now() +
     (LEAST(3600,30 * power(2,LEAST($5 - 1,7))) * interval '1 second')
     ELSE available_at END,
   lease_owner=NULL,lease_expires_at=NULL,
   updated_at=now()
   WHERE id=$1 AND status='running' AND lease_owner=$6"#;

struct ClaimedJobLease {
    job_id: Uuid,
    owner: String,
}

enum LeasedExecution<T> {
    Finished(T),
    Lost,
}

async fn execute_with_job_lease<F, T>(
    pool: &PgPool,
    lease: &ClaimedJobLease,
    execution: F,
) -> Result<LeasedExecution<T>, ApiError>
where
    F: std::future::Future<Output = T>,
{
    let mut renewal = time::interval_at(
        time::Instant::now() + JOB_LEASE_RENEW_INTERVAL,
        JOB_LEASE_RENEW_INTERVAL,
    );
    tokio::pin!(execution);
    loop {
        tokio::select! {
            output = &mut execution => return Ok(LeasedExecution::Finished(output)),
            _ = renewal.tick() => {
                // Dropping the execution future is the safest response when
                // another worker has reclaimed the durable job.
                if !renew_job_lease(pool, lease).await? {
                    return Ok(LeasedExecution::Lost);
                }
            }
        }
    }
}

async fn renew_job_lease(pool: &PgPool, lease: &ClaimedJobLease) -> Result<bool, ApiError> {
    let mut transaction = pool.begin().await?;
    let updated = sqlx::query(RENEW_JOB_LEASE_SQL)
        .bind(lease.job_id)
        .bind(&lease.owner)
        .bind(JOB_LEASE_SECONDS)
        .execute(&mut *transaction)
        .await?;
    if updated.rows_affected() != 1 {
        transaction.rollback().await?;
        return Ok(false);
    }
    sqlx::query("UPDATE operation_runs SET updated_at=now() WHERE id=$1 AND status='running'")
        .bind(lease.job_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(true)
}

async fn persist_job_success(
    pool: &PgPool,
    lease: &ClaimedJobLease,
    output: &Value,
) -> Result<bool, ApiError> {
    let mut transaction = pool.begin().await?;
    // This must remain the first mutation in the transaction. Every related
    // status write below is committed only after the owner fence succeeds.
    let updated = sqlx::query(COMPLETE_JOB_SQL)
        .bind(lease.job_id)
        .bind(output)
        .bind(&lease.owner)
        .execute(&mut *transaction)
        .await?;
    if updated.rows_affected() != 1 {
        transaction.rollback().await?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE operation_runs SET status='completed', result=$2, updated_at=now() WHERE id=$1",
    )
    .bind(lease.job_id)
    .bind(output)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(true)
}

async fn persist_job_failure(
    pool: &PgPool,
    lease: &ClaimedJobLease,
    job_type: &str,
    attempts: i32,
    max_attempts: i32,
    detail: &str,
) -> Result<bool, ApiError> {
    let output = json!({"error": detail});
    let retrying = attempts < max_attempts;
    let status = if retrying { "queued" } else { "failed" };
    let operation_status = if retrying { "queued" } else { "failed" };
    let mut transaction = pool.begin().await?;
    // As with success, retry/dead-letter state is one owner-fenced unit with
    // all of the job type's related operation state.
    let updated = sqlx::query(FAIL_JOB_SQL)
        .bind(lease.job_id)
        .bind(status)
        .bind(&output)
        .bind(detail)
        .bind(attempts)
        .bind(&lease.owner)
        .execute(&mut *transaction)
        .await?;
    if updated.rows_affected() != 1 {
        transaction.rollback().await?;
        return Ok(false);
    }

    sqlx::query("UPDATE operation_runs SET status=$2, result=$3, updated_at=now() WHERE id=$1")
        .bind(lease.job_id)
        .bind(operation_status)
        .bind(&output)
        .execute(&mut *transaction)
        .await?;
    persist_related_failure(
        &mut transaction,
        lease.job_id,
        job_type,
        retrying,
    )
    .await?;
    transaction.commit().await?;
    Ok(true)
}

async fn persist_related_failure(
    transaction: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    job_type: &str,
    retrying: bool,
) -> Result<(), ApiError> {
    if job_type == "productImport" {
        sqlx::query(
            r#"UPDATE product_import_runs
               SET status=CASE WHEN $2 THEN 'readyToPublish' ELSE 'failed' END,
                   completed_at=CASE WHEN $2 THEN NULL ELSE now() END
               WHERE id=$1 AND status <> 'completed'"#,
        )
        .bind(job_id)
        .bind(retrying)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}
