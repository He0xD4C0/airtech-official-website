use sqlx::Row;

use airtek_domain::models::{CursorPage, ProductSourceMetadata, SourceMetadataKind};

use crate::error::ApiError;
use crate::pagination::{paginate_by_id_scoped, CursorQuery};
use crate::state::AppState;

pub async fn list_source_metadata(
    state: &AppState,
    kind: Option<SourceMetadataKind>,
    query_text: Option<String>,
    pagination: CursorQuery,
) -> Result<CursorPage<ProductSourceMetadata>, ApiError> {
    let query_text = query_text
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if query_text
        .as_ref()
        .is_some_and(|value| value.chars().count() > 200)
    {
        return Err(ApiError::bad_request("q must not exceed 200 characters."));
    }
    let kind_filter = kind.as_ref().map(SourceMetadataKind::as_str);
    let rows = sqlx::query(
        r#"SELECT id,kind,label,source_table,source_record_id,archive_sha256,
                  attributes,raw_fields,captured_at,imported_at
           FROM product_source_metadata
           WHERE ($1::text IS NULL OR kind=$1)
             AND (
               $2::text IS NULL
               OR label ILIKE '%' || $2 || '%'
               OR source_record_id ILIKE '%' || $2 || '%'
             )
           ORDER BY label,id"#,
    )
    .bind(kind_filter)
    .bind(query_text.as_deref())
    .fetch_all(&state.pool)
    .await?;
    let values = rows
        .into_iter()
        .map(|row| {
            let raw_kind: String = row.try_get("kind")?;
            Ok(ProductSourceMetadata {
                id: row.try_get("id")?,
                kind: SourceMetadataKind::parse(&raw_kind)
                    .ok_or_else(|| ApiError::internal("Stored source metadata kind is invalid."))?,
                label: row.try_get("label")?,
                source_table: row.try_get("source_table")?,
                source_record_id: row.try_get("source_record_id")?,
                archive_sha256: row.try_get("archive_sha256")?,
                attributes: row.try_get("attributes")?,
                raw_fields: row.try_get("raw_fields")?,
                captured_at: row.try_get("captured_at")?,
                imported_at: row.try_get("imported_at")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let scope = format!(
        "product-source-metadata:{}:{}",
        kind_filter.unwrap_or("all"),
        query_text.as_deref().unwrap_or("all")
    );
    paginate_by_id_scoped(&scope, values, pagination, |item| item.id)
}
