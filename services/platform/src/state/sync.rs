use super::*;

impl AppState {
    pub async fn persist_sync_run(&self, run: &SyncRun) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO sync_runs
               (id, source, dry_run, mapping_version, status, resume_cursor, records_seen,
                records_valid, conflict_count, started_at, completed_at, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
               ON CONFLICT (id) DO UPDATE SET status=EXCLUDED.status,
               resume_cursor=EXCLUDED.resume_cursor, records_seen=EXCLUDED.records_seen,
               records_valid=EXCLUDED.records_valid, conflict_count=EXCLUDED.conflict_count,
               completed_at=EXCLUDED.completed_at, payload=EXCLUDED.payload"#,
        )
        .bind(run.id)
        .bind(&run.source)
        .bind(run.dry_run)
        .bind(&run.mapping_version)
        .bind(enum_label(run.status))
        .bind(&run.resume_cursor)
        .bind(run.records_seen as i64)
        .bind(run.records_valid as i64)
        .bind(run.conflict_count as i64)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(
            serde_json::to_value(run)
                .map_err(|_| ApiError::internal("Sync serialization failed."))?,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn enqueue_sync_run(&self, run: &SyncRun) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let payload = serde_json::to_value(run)
            .map_err(|_| ApiError::internal("Sync serialization failed."))?;
        let job_payload = serde_json::json!({
            "syncRunId": run.id,
            "dryRun": run.dry_run,
            "mappingVersion": run.mapping_version,
            "cursor": run.resume_cursor,
        });
        let mut transaction = pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO sync_runs
               (id, source, dry_run, mapping_version, status, resume_cursor, records_seen,
                records_valid, conflict_count, started_at, completed_at, payload)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        )
        .bind(run.id)
        .bind(&run.source)
        .bind(run.dry_run)
        .bind(&run.mapping_version)
        .bind(enum_label(run.status))
        .bind(&run.resume_cursor)
        .bind(run.records_seen as i64)
        .bind(run.records_valid as i64)
        .bind(run.conflict_count as i64)
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(&payload)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO jobs
               (id, job_type, status, payload, available_at, created_at, updated_at)
               VALUES ($1,'feishuSync','queued',$2,$3,$3,$3)"#,
        )
        .bind(run.id)
        .bind(job_payload)
        .bind(run.started_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn list_sync_runs(&self) -> Result<Vec<SyncRun>, ApiError> {
        if let Some(pool) = &self.pool {
            let rows = sqlx::query("SELECT payload FROM sync_runs ORDER BY started_at DESC")
                .fetch_all(pool)
                .await?;
            return rows
                .into_iter()
                .map(|row| decode_payload(row.try_get("payload")?, "sync run"))
                .collect();
        }
        let mut values: Vec<_> = self.data.read().await.sync_runs.values().cloned().collect();
        values.sort_by_key(|run| std::cmp::Reverse(run.started_at));
        Ok(values)
    }
}
