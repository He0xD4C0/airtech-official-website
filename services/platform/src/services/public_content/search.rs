use super::discovery::{canonical_patterns, DISCOVERY_SQL};
use crate::error::ApiError;
use crate::models::{
    ProductFacetCount, PublicSearchEntityType, PublicSearchItem, PublicSearchPage,
    PublicSearchQuery, PublicSearchType,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchCursor {
    version: u8,
    path: String,
    id: Uuid,
    q: Option<String>,
    content_type: Option<PublicSearchType>,
}

pub(crate) async fn search_public_site(
    pool: &sqlx::PgPool,
    query: PublicSearchQuery,
) -> Result<PublicSearchPage, ApiError> {
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let q = normalize_query(query.q.as_deref())?;
    let after = query
        .cursor
        .as_deref()
        .map(|value| decode_cursor(value, q.as_deref(), query.content_type))
        .transpose()?;
    let matched = format!(
        r#"WITH eligible AS ({DISCOVERY_SQL}), matched AS (
        SELECT * FROM eligible WHERE $2::text IS NULL OR NOT EXISTS (
            SELECT 1 FROM unnest(string_to_array($2,' ')) term
            WHERE strpos(lower(concat_ws(' ',title,summary,canonical_path,display_type)),term)=0))"#
    );
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let counts = sqlx::query(&format!("{matched} SELECT display_type,count(*) AS count FROM matched GROUP BY display_type ORDER BY display_type"))
        .bind(canonical_patterns()).bind(&q).fetch_all(&mut *transaction).await?;
    let type_counts = counts
        .into_iter()
        .map(|row| {
            Ok(ProductFacetCount {
                value: row.try_get("display_type")?,
                count: usize::try_from(row.try_get::<i64, _>("count")?).unwrap_or(usize::MAX),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let total = type_counts
        .iter()
        .filter(|facet| {
            query
                .content_type
                .is_none_or(|kind| kind.label() == facet.value)
        })
        .map(|facet| facet.count)
        .sum();
    let rows = sqlx::query(&format!(
        r#"{matched} SELECT * FROM matched
        WHERE ($3::text IS NULL OR display_type=$3)
          AND ($4::text IS NULL OR (canonical_path,entity_id)>($4,$5))
        ORDER BY canonical_path,entity_id LIMIT $6"#
    ))
    .bind(canonical_patterns())
    .bind(&q)
    .bind(query.content_type.map(|kind| kind.label()))
    .bind(after.as_ref().map(|value| &value.path))
    .bind(after.as_ref().map(|value| value.id))
    .bind((limit + 1) as i64)
    .fetch_all(&mut *transaction)
    .await?;
    transaction.commit().await?;
    let mut items = rows
        .into_iter()
        .map(|row| {
            Ok(PublicSearchItem {
                entity_type: if row.try_get::<String, _>("entity_type")? == "product" {
                    PublicSearchEntityType::Product
                } else {
                    PublicSearchEntityType::Content
                },
                entity_id: row.try_get("entity_id")?,
                title: row.try_get("title")?,
                summary: row.try_get("summary")?,
                canonical_path: row.try_get("canonical_path")?,
                display_type: serde_json::from_value(serde_json::Value::String(
                    row.try_get("display_type")?,
                ))
                .map_err(|_| ApiError::service_unavailable("Published search type is invalid."))?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let has_more = items.len() > limit;
    items.truncate(limit);
    let next_cursor = has_more
        .then(|| items.last())
        .flatten()
        .map(|entry| encode_cursor(entry, q.clone(), query.content_type))
        .transpose()?;
    Ok(PublicSearchPage {
        items,
        next_cursor,
        total,
        type_counts,
    })
}

fn normalize_query(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Ok(None);
    }
    if normalized.len() > 200 || normalized.chars().any(char::is_control) {
        return Err(ApiError::bad_request(
            "q must contain at most 200 printable characters.",
        ));
    }
    Ok(Some(normalized.to_lowercase()))
}

fn encode_cursor(
    entry: &PublicSearchItem,
    q: Option<String>,
    content_type: Option<PublicSearchType>,
) -> Result<String, ApiError> {
    serde_json::to_vec(&SearchCursor {
        version: 1,
        path: entry.canonical_path.clone(),
        id: entry.entity_id,
        q,
        content_type,
    })
    .map(|value| URL_SAFE_NO_PAD.encode(value))
    .map_err(|_| ApiError::internal("Search cursor serialization failed."))
}

fn decode_cursor(
    value: &str,
    q: Option<&str>,
    content_type: Option<PublicSearchType>,
) -> Result<SearchCursor, ApiError> {
    if value.is_empty() || value.len() > 2_048 {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<SearchCursor>(&value).ok())
        .filter(|cursor| {
            cursor.version == 1
                && (cursor.path == "/en" || cursor.path.starts_with("/en/"))
                && cursor.path.len() <= 2_048
                && cursor.q.as_deref() == q
                && cursor.content_type == content_type
        })
        .ok_or_else(|| {
            ApiError::bad_request("cursor is invalid or belongs to a different search query.")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_cursor_is_bound_to_normalized_query_and_type() {
        let entry = PublicSearchItem {
            entity_type: PublicSearchEntityType::Product,
            entity_id: Uuid::new_v4(),
            title: "Published model".into(),
            summary: None,
            canonical_path: "/en/products/axial/published-model".into(),
            display_type: PublicSearchType::Product,
        };
        let cursor = encode_cursor(
            &entry,
            Some("published model".into()),
            Some(PublicSearchType::Product),
        )
        .unwrap();
        assert!(decode_cursor(
            &cursor,
            Some("published model"),
            Some(PublicSearchType::Product)
        )
        .is_ok());
        assert!(
            decode_cursor(&cursor, Some("different"), Some(PublicSearchType::Product)).is_err()
        );
    }
}
