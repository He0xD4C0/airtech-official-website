use super::*;

impl AppState {
    pub async fn product_publication_issues(
        &self,
        product: &Product,
    ) -> Result<Vec<ValidationIssue>, ApiError> {
        self.postgres_product_publication_issues(product).await
    }

    pub async fn assert_product_publishable(&self, product: &Product) -> Result<(), ApiError> {
        let issues = self.product_publication_issues(product).await?;
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ApiError::validation(issues_as_errors(issues)))
        }
    }

    /// Presentation publication has its own revision clock. A Product Master
    /// fact revision can remain current while an editor has a newer website
    /// draft that still needs to be projected publicly.
    pub async fn product_presentation_is_published(
        &self,
        product_id: Uuid,
        locale: &str,
    ) -> Result<bool, ApiError> {
        Ok(sqlx::query_scalar::<_, bool>(
            r#"SELECT published_revision=current_revision
               FROM product_presentation_working
               WHERE product_id=$1 AND locale=$2"#,
        )
        .bind(product_id)
        .bind(locale)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(false))
    }
}
