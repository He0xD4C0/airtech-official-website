use std::time::Duration;

use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use tokio::time;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

pub async fn run(state: AppState) -> Result<(), ApiError> {
    let Some(pool) = state.pool.clone() else {
        tracing::warn!("worker is running without PostgreSQL; no durable jobs can be claimed");
        tokio::signal::ctrl_c()
            .await
            .map_err(|_| ApiError::internal("Unable to install shutdown signal."))?;
        return Ok(());
    };

    let mut interval = time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Ok(()),
            _ = interval.tick() => {
                if let Err(error) = claim_outbox_event(&pool).await {
                    tracing::error!(error = %error, "outbox poll failed");
                }
                if let Err(error) = claim_and_run(&pool).await {
                    tracing::error!(error = %error, "background job poll failed");
                }
            }
        }
    }
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

    // Provider-specific CDN/search/sitemap adapters remain deployment
    // configuration. Until one is actually present, retain retry semantics and
    // never report that a publication side effect completed.
    if matches!(
        topic.as_str(),
        "public.content.published" | "public.product.published"
    ) {
        tracing::warn!(%id, %topic, %aggregate_id, payload = %payload, "public projection adapter is not configured");
    } else {
        tracing::error!(%id, %topic, "unknown outbox topic");
    }
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

async fn claim_and_run(pool: &PgPool) -> Result<(), ApiError> {
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

    let result = execute_job(pool, &job_type, payload).await;
    match result {
        Ok(output) => {
            sqlx::query(
                "UPDATE jobs SET status='completed', result=$2, updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(&output)
            .execute(pool)
            .await?;
            sqlx::query(
                "UPDATE operation_runs SET status='completed', result=$2, updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(output)
            .execute(pool)
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
                .execute(pool)
                .await?;
            }
        }
        Err(detail) => {
            let output = json!({"error": detail});
            let retrying = attempts < max_attempts;
            let status = if retrying { "queued" } else { "failed" };
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
            .execute(pool)
            .await?;
            sqlx::query(
                "UPDATE operation_runs SET status=$2, result=$3, updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .bind(status)
            .bind(output)
            .execute(pool)
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
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(())
}

async fn execute_job(pool: &PgPool, job_type: &str, _payload: Value) -> Result<Value, String> {
    match job_type {
        "migrationPreflight" => {
            let version = sqlx::query_scalar::<_, String>("SHOW server_version")
                .fetch_one(pool)
                .await
                .map_err(|error| error.to_string())?;
            Ok(json!({"checkedAt": Utc::now(), "serverVersion": version}))
        }
        "migrationApply" => {
            sqlx::migrate!("./migrations")
                .run(pool)
                .await
                .map_err(|error| error.to_string())?;
            Ok(json!({"migratedAt": Utc::now()}))
        }
        "retentionApply" => {
            let grace_days = integer_setting(
                pool,
                "retentionDeletionGraceDays",
                30,
                1,
                365,
            )
            .await?;
            let queued_rfqs = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM rfq_submissions WHERE retention_until < now() AND retention_until >= now() - make_interval(days => $1)",
            )
            .bind(grace_days as i32)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?;
            let queued_contacts = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM contact_requests WHERE retention_until < now() AND retention_until >= now() - make_interval(days => $1)",
            )
            .bind(grace_days as i32)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?;
            let rfqs = sqlx::query(
                r#"UPDATE rfq_submissions
                   SET status='piiCleared',
                       payload=jsonb_set(
                           jsonb_set(
                               jsonb_set(payload, '{request,contact}',
                                   '{"name":"[retention-cleared]","email":"[retention-cleared]","phone":null,"company":null,"countryOrRegion":null}'::jsonb, true),
                               '{request,context}', '{}'::jsonb, true),
                           '{status}', '"piiCleared"'::jsonb, true)
                   WHERE retention_until < now() - make_interval(days => $1)
                     AND status <> 'piiCleared'"#,
            )
                .bind(grace_days as i32)
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?
                .rows_affected();
            let contacts = sqlx::query(
                r#"UPDATE contact_requests
                   SET status='piiCleared',
                       payload=jsonb_set(
                           jsonb_set(
                               jsonb_set(payload, '{request,contact}',
                                   '{"name":"[retention-cleared]","email":"[retention-cleared]","phone":null,"company":null,"countryOrRegion":null}'::jsonb, true),
                               '{request,message}', '"[retention-cleared]"'::jsonb, true),
                           '{status}', '"piiCleared"'::jsonb, true)
                   WHERE retention_until < now() - make_interval(days => $1)
                     AND status <> 'piiCleared'"#,
            )
                .bind(grace_days as i32)
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?
                .rows_affected();
            Ok(json!({
                "gracePeriodDays": grace_days,
                "rfqsQueuedForPiiClearing": queued_rfqs,
                "contactsQueuedForPiiClearing": queued_contacts,
                "rfqsPiiCleared": rfqs,
                "contactsPiiCleared": contacts
            }))
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

async fn integer_setting(
    pool: &PgPool,
    key: &str,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, String> {
    let value = sqlx::query_scalar::<_, Option<Value>>(
        "SELECT (SELECT value FROM app_settings WHERE key=$1)",
    )
    .bind(key)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())?
    .unwrap_or(Value::from(default));
    value
        .as_i64()
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or_else(|| format!("{key} must be an integer between {minimum} and {maximum}"))
}
