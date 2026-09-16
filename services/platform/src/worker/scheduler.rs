use super::*;

pub(super) const RETENTION_SCHEDULE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
pub(super) const RETENTION_JOB_CADENCE_HOURS: i64 = 24;

pub async fn run(state: AppState) -> Result<(), ApiError> {
    let pool = state.pool.clone();

    ensure_periodic_retention_job(&pool).await?;
    let mut interval = time::interval(Duration::from_secs(2));
    let mut retention_schedule_interval = time::interval_at(
        time::Instant::now() + RETENTION_SCHEDULE_CHECK_INTERVAL,
        RETENTION_SCHEDULE_CHECK_INTERVAL,
    );
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = retention_schedule_interval.tick() => {
                if let Err(error) = ensure_periodic_retention_job(&pool).await {
                    tracing::error!(error = %error, "retention scheduling failed");
                }
            }
            _ = interval.tick() => {
                if let Err(error) = claim_outbox_event(&pool).await {
                    tracing::error!(error = %error, "outbox poll failed");
                }
                if let Err(error) = run_one_pending_job(&state).await {
                    tracing::error!(error = %error, "background job poll failed");
                }
            }
        }
    }
}

/// Ensure there is at most one daily retention operation across all worker
/// instances. The database advisory lock makes the check-and-insert atomic
/// without process-local state or an external scheduler.
pub async fn ensure_periodic_retention_job(pool: &PgPool) -> Result<Option<Uuid>, ApiError> {
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('airtek.retention.schedule', 0))")
        .execute(&mut *transaction)
        .await?;
    let previous = sqlx::query(
        r#"SELECT status,created_at FROM jobs
           WHERE job_type='retentionApply'
           ORDER BY (status IN ('queued','running')) DESC,created_at DESC
           LIMIT 1"#,
    )
    .fetch_optional(&mut *transaction)
    .await?
    .map(|row| {
        Ok::<_, sqlx::Error>((
            row.try_get::<String, _>("status")?,
            row.try_get::<DateTime<Utc>, _>("created_at")?,
        ))
    })
    .transpose()?;
    if !retention_job_is_due(
        previous.as_ref().map(|(status, at)| (status.as_str(), *at)),
        now,
    ) {
        transaction.commit().await?;
        return Ok(None);
    }

    let id = Uuid::new_v4();
    let reason = "Scheduled daily privacy retention enforcement";
    sqlx::query(
        r#"INSERT INTO operation_runs(id,kind,status,reason,result,created_at,updated_at)
           VALUES ($1,'retentionApply','queued',$2,NULL,$3,$3)"#,
    )
    .bind(id)
    .bind(reason)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO jobs(id,job_type,status,payload,available_at,created_at,updated_at)
           VALUES ($1,'retentionApply','queued',$2,$3,$3,$3)"#,
    )
    .bind(id)
    .bind(json!({"trigger": "periodic", "cadenceHours": RETENTION_JOB_CADENCE_HOURS}))
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Some(id))
}

pub(super) fn retention_job_is_due(
    previous: Option<(&str, DateTime<Utc>)>,
    now: DateTime<Utc>,
) -> bool {
    match previous {
        None => true,
        Some(("queued" | "running", _)) => false,
        Some((_, created_at)) => {
            created_at <= now - chrono::Duration::hours(RETENTION_JOB_CADENCE_HOURS)
        }
    }
}
