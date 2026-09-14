use super::*;

impl AppState {
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
