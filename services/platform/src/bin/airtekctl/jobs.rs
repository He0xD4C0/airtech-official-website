async fn list_jobs(pool: &PgPool, limit: u16) -> Result<(), Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT id, job_type, status, attempts, max_attempts, available_at, last_error, created_at, updated_at FROM jobs ORDER BY created_at DESC LIMIT $1",
    )
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    let jobs: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<Uuid, _>("id")?,
                "jobType": row.try_get::<String, _>("job_type")?,
                "status": row.try_get::<String, _>("status")?,
                "attempts": row.try_get::<i32, _>("attempts")?,
                "maxAttempts": row.try_get::<i32, _>("max_attempts")?,
                "availableAt": row.try_get::<chrono::DateTime<chrono::Utc>, _>("available_at")?,
                "lastError": row.try_get::<Option<String>, _>("last_error")?,
                "createdAt": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
                "updatedAt": row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at")?,
            }))
        })
        .collect::<Result<_, sqlx::Error>>()?;
    print_json(json!({"items": jobs}))?;
    Ok(())
}

async fn retry_job(pool: &PgPool, id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
    let mut transaction = pool.begin().await?;
    let job_type = sqlx::query_scalar::<_, String>(
        "UPDATE jobs SET status='queued', attempts=0, result=NULL, available_at=now(), last_error=NULL, updated_at=now() WHERE id=$1 AND status='failed' RETURNING job_type",
    )
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or("Only a failed job can be retried")?;
    sqlx::query(
        "UPDATE operation_runs SET status='queued', result=NULL, updated_at=now() WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *transaction)
    .await?;
    if job_type == "feishuSync" {
        sqlx::query(
            "UPDATE sync_runs SET status='queued', completed_at=NULL, payload=(jsonb_set(payload, '{status}', '\"queued\"', true) - 'error' - 'completedAt') WHERE id=$1",
        )
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }
    if job_type == "productImport" {
        reset_staged_product_import_for_retry(&mut transaction, id).await?;
    }
    insert_cli_audit(
        &mut transaction,
        "devtools.cli.job.retried",
        "backgroundJob",
        Some(id),
        json!({"jobType": &job_type, "attemptsReset": true}),
        "Failed development job queued with a fresh bounded attempt budget.",
    )
    .await?;
    transaction.commit().await?;
    print_json(json!({
        "status": "queued",
        "jobId": id,
        "jobType": job_type,
        "note": "The retry was queued; it has not completed.",
    }))?;
    Ok(())
}
