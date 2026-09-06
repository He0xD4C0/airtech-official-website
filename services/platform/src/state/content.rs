use super::*;

impl AppState {
    /// Reads only historical legacy preview payloads. Unified CMS mutations
    /// live exclusively in `services::cms_content` and never use this adapter.
    pub async fn load_content_revision(
        &self,
        content_id: Uuid,
        revision: i64,
    ) -> Result<Option<ContentEntry>, ApiError> {
        if let Some(pool) = &self.pool {
            let payload = sqlx::query_scalar::<_, Value>(
                r#"SELECT payload FROM content_revisions
                   WHERE content_id=$1 AND revision=$2 AND payload IS NOT NULL"#,
            )
            .bind(content_id)
            .bind(revision)
            .fetch_optional(pool)
            .await?;
            return payload
                .map(|payload| decode_payload(payload, "legacy content preview revision"))
                .transpose();
        }

        Ok(self
            .data
            .read()
            .await
            .content_revisions
            .get(&content_id)
            .and_then(|revisions| revisions.get(&revision))
            .cloned())
    }
}
