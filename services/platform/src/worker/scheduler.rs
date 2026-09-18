use super::*;

pub(super) const RETENTION_SCHEDULE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
pub(super) const FEISHU_SCHEDULE_CHECK_INTERVAL: Duration = Duration::from_secs(60);
pub(super) const RETENTION_JOB_CADENCE_HOURS: i64 = 24;

pub async fn run(state: AppState) -> Result<(), ApiError> {
    let pool = state.pool.clone();

    ensure_periodic_retention_job(&pool).await?;
    ensure_scheduled_feishu_sync(&state).await?;
    let mut interval = time::interval(Duration::from_secs(2));
    let mut retention_schedule_interval = time::interval_at(
        time::Instant::now() + RETENTION_SCHEDULE_CHECK_INTERVAL,
        RETENTION_SCHEDULE_CHECK_INTERVAL,
    );
    let mut feishu_schedule_interval = time::interval_at(
        time::Instant::now() + FEISHU_SCHEDULE_CHECK_INTERVAL,
        FEISHU_SCHEDULE_CHECK_INTERVAL,
    );
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = retention_schedule_interval.tick() => {
                if let Err(error) = ensure_periodic_retention_job(&pool).await {
                    tracing::error!(error = %error, "retention scheduling failed");
                }
            }
            _ = feishu_schedule_interval.tick() => {
                if let Err(error) = ensure_scheduled_feishu_sync(&state).await {
                    tracing::error!(error = %error, "Feishu scheduling failed");
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

pub async fn ensure_scheduled_feishu_sync(state: &AppState) -> Result<Option<Uuid>, ApiError> {
    let settings = crate::services::feishu::get_feishu_settings(state).await?;
    if !settings.enabled {
        return Ok(None);
    }
    let active: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM sync_runs WHERE connector_id=$1
           AND status IN ('queued','fetching','validating','readyToPublish'))"#,
    )
    .bind(settings.connector_id)
    .fetch_one(&state.pool)
    .await?;
    if active {
        return Ok(None);
    }
    let now = Utc::now();
    let shanghai =
        chrono::FixedOffset::east_opt(8 * 60 * 60).expect("Asia/Shanghai fixed offset is valid");
    let local_now = now.with_timezone(&shanghai);
    let full_time = chrono::NaiveTime::parse_from_str(&settings.full_reconcile_local_time, "%H:%M")
        .map_err(|_| ApiError::service_unavailable("Stored Feishu full-sync time is invalid."))?;
    let full_due = settings.full_reconcile_enabled
        && local_now.time() >= full_time
        && settings
            .last_full_at
            .map(|value| value.with_timezone(&shanghai).date_naive() < local_now.date_naive())
            .unwrap_or(true);
    let last_automatic = match (settings.last_incremental_at, settings.last_full_at) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (left, right) => left.or(right),
    };
    let incremental_due = last_automatic
        .map(|value| value <= now - chrono::Duration::minutes(i64::from(settings.interval_minutes)))
        .unwrap_or(true);
    let run_kind = if full_due {
        crate::models::SyncRunKind::Full
    } else if incremental_due {
        crate::models::SyncRunKind::Incremental
    } else {
        return Ok(None);
    };
    let run = crate::services::feishu::queue_sync_run(state, run_kind, "feishuScheduler").await?;
    Ok(Some(run.id))
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
