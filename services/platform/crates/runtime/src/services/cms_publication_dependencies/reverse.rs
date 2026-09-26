use sqlx::{postgres::PgConnection, Row};
use uuid::Uuid;

/// One currently published CMS revision that points at the content being
/// unpublished. The JSON Pointer is retained so an editor can repair the
/// exact source field before retrying.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveContentDependent {
    pub source_content_id: Uuid,
    pub source_revision: i64,
    pub reference_path: String,
}

pub async fn active_content_dependents(
    connection: &mut PgConnection,
    target_content_id: Uuid,
) -> Result<Vec<ActiveContentDependent>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT dependency.source_content_id,
                  published.publication_version AS source_revision,
                  dependency.reference_path
           FROM cms_current_publication_dependencies dependency
           JOIN cms_published_content published
             ON published.content_id=dependency.source_content_id
           WHERE dependency.target_content_id=$1
             AND dependency.source_content_id<>$1
           ORDER BY dependency.source_content_id,
                    published.publication_version,
                    dependency.reference_path
           FOR KEY SHARE OF published"#,
    )
    .bind(target_content_id)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ActiveContentDependent {
                source_content_id: row.try_get("source_content_id")?,
                source_revision: row.try_get("source_revision")?,
                reference_path: row.try_get("reference_path")?,
            })
        })
        .collect()
}
