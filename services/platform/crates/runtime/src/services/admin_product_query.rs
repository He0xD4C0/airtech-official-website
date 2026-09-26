use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiError;
use crate::pagination::{
    cursor_limit, decode_scoped_cursor_compat, encode_scoped_cursor, CursorQuery, DecodedCursor,
};
use crate::services::request_metrics::LegacyCursorEndpoint;
use crate::state::{decode_payload, overlay_presentation_row, AppState};
use airtek_domain::models::{AdminProductPage, Product, ProductFacetCount};

#[derive(Debug)]
pub struct ProductListFilter {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    pub search: Option<String>,
    pub family: Option<String>,
    pub status: Option<String>,
    pub data_state: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductCursor {
    stable_id: String,
    id: Uuid,
}

pub async fn list_admin_products(
    state: &AppState,
    filter: ProductListFilter,
) -> Result<AdminProductPage, ApiError> {
    let scope = format!(
        "admin.products|{:?}|{:?}|{:?}|{:?}",
        filter.search, filter.family, filter.status, filter.data_state
    );
    let pagination = CursorQuery {
        cursor: filter.cursor.clone(),
        limit: filter.limit,
    };
    let limit = cursor_limit(&pagination)?;
    let after = decode_product_cursor(state, &scope, &filter, pagination.cursor.as_deref()).await?;
    let pending = filter.data_state.as_deref().map(|value| value == "pending");
    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM products product
           LEFT JOIN product_presentation_working presentation
             ON presentation.product_id=product.id AND presentation.locale=product.locale
           WHERE ($1::text IS NULL OR product.stable_id ILIKE '%'||$1||'%'
                  OR product.model ILIKE '%'||$1||'%'
                  OR presentation.title ILIKE '%'||$1||'%'
                  OR CASE product.family
                       WHEN 'centrifugal' THEN 'centrifugal fans'
                       WHEN 'axial' THEN 'axial fans'
                       WHEN 'crossFlow' THEN 'crossflow cross-flow fans'
                       WHEN 'inlineDuct' THEN 'inlineduct inline duct fans'
                       WHEN 'motors' THEN 'motors'
                     END ILIKE '%'||$1||'%')
             AND ($2::text IS NULL OR product.family=$2)
             AND ($3::text IS NULL OR product.status=$3)
             AND ($4::boolean IS NULL OR $4 = EXISTS(
                   SELECT 1 FROM product_specs specification
                   WHERE specification.product_id=product.id
                     AND specification.product_revision=product.current_revision
                     AND specification.fact_state='pendingVerification'))"#,
    )
    .bind(filter.search.as_deref())
    .bind(filter.family.as_deref())
    .bind(filter.status.as_deref())
    .bind(pending)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query(
        r#"SELECT product.payload,
                  presentation.current_revision AS presentation_revision,
                  presentation.locale AS presentation_locale,
                  presentation.slug AS presentation_slug,
                  presentation.title AS presentation_title,
                  presentation.summary AS presentation_summary,
                  presentation.content AS presentation_content,
                  presentation.seo_metadata AS presentation_seo,
                  presentation.indexable AS presentation_indexable,
                  product.stable_id AS cursor_stable_id,product.id AS cursor_id
           FROM products product
           LEFT JOIN product_presentation_working presentation
             ON presentation.product_id=product.id AND presentation.locale=product.locale
           WHERE ($1::text IS NULL OR product.stable_id ILIKE '%'||$1||'%'
                  OR product.model ILIKE '%'||$1||'%'
                  OR presentation.title ILIKE '%'||$1||'%'
                  OR CASE product.family
                       WHEN 'centrifugal' THEN 'centrifugal fans'
                       WHEN 'axial' THEN 'axial fans'
                       WHEN 'crossFlow' THEN 'crossflow cross-flow fans'
                       WHEN 'inlineDuct' THEN 'inlineduct inline duct fans'
                       WHEN 'motors' THEN 'motors'
                     END ILIKE '%'||$1||'%')
             AND ($2::text IS NULL OR product.family=$2)
             AND ($3::text IS NULL OR product.status=$3)
             AND ($4::boolean IS NULL OR $4 = EXISTS(
                   SELECT 1 FROM product_specs specification
                   WHERE specification.product_id=product.id
                     AND specification.product_revision=product.current_revision
                     AND specification.fact_state='pendingVerification'))
             AND ($5::text IS NULL OR (product.stable_id,product.id)>($5,$6))
           ORDER BY product.stable_id,product.id LIMIT $7"#,
    )
    .bind(filter.search.as_deref())
    .bind(filter.family.as_deref())
    .bind(filter.status.as_deref())
    .bind(pending)
    .bind(after.as_ref().map(|value| value.stable_id.as_str()))
    .bind(after.as_ref().map(|value| value.id))
    .bind(i64::try_from(limit + 1).unwrap_or(101))
    .fetch_all(&state.pool)
    .await?;
    let mut products = Vec::with_capacity(rows.len());
    let mut positions = Vec::with_capacity(rows.len());
    for row in rows {
        let mut product: Product = decode_payload(row.try_get("payload")?, "product")?;
        overlay_presentation_row(&mut product, &row)?;
        positions.push(ProductCursor {
            stable_id: row.try_get("cursor_stable_id")?,
            id: row.try_get("cursor_id")?,
        });
        products.push(product);
    }
    let has_more = products.len() > limit;
    products.truncate(limit);
    positions.truncate(limit);
    let next_cursor = if has_more {
        positions
            .last()
            .map(|position| encode_scoped_cursor(&scope, position))
            .transpose()?
    } else {
        None
    };
    let (family_counts, status_counts, data_state_counts) = load_facets(state, &filter).await?;
    Ok(AdminProductPage {
        items: products,
        next_cursor,
        total: usize::try_from(total).unwrap_or(usize::MAX),
        family_counts,
        status_counts,
        data_state_counts,
    })
}

async fn decode_product_cursor(
    state: &AppState,
    scope: &str,
    filter: &ProductListFilter,
    cursor: Option<&str>,
) -> Result<Option<ProductCursor>, ApiError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    match decode_scoped_cursor_compat::<Uuid, ProductCursor>(scope, cursor)? {
        DecodedCursor::Current(position) => Ok(Some(position)),
        DecodedCursor::Legacy(legacy_id) => {
            let pending = filter.data_state.as_deref().map(|value| value == "pending");
            let stable_id = sqlx::query_scalar::<_, String>(
                r#"SELECT product.stable_id FROM products product
                   LEFT JOIN product_presentation_working presentation
                     ON presentation.product_id=product.id AND presentation.locale=product.locale
                   WHERE product.id=$1
                     AND ($2::text IS NULL OR product.stable_id ILIKE '%'||$2||'%'
                          OR product.model ILIKE '%'||$2||'%'
                          OR presentation.title ILIKE '%'||$2||'%'
                          OR CASE product.family
                               WHEN 'centrifugal' THEN 'centrifugal fans'
                               WHEN 'axial' THEN 'axial fans'
                               WHEN 'crossFlow' THEN 'crossflow cross-flow fans'
                               WHEN 'inlineDuct' THEN 'inlineduct inline duct fans'
                               WHEN 'motors' THEN 'motors'
                             END ILIKE '%'||$2||'%')
                     AND ($3::text IS NULL OR product.family=$3)
                     AND ($4::text IS NULL OR product.status=$4)
                     AND ($5::boolean IS NULL OR $5 = EXISTS(
                           SELECT 1 FROM product_specs specification
                           WHERE specification.product_id=product.id
                             AND specification.product_revision=product.current_revision
                             AND specification.fact_state='pendingVerification'))"#,
            )
            .bind(legacy_id)
            .bind(filter.search.as_deref())
            .bind(filter.family.as_deref())
            .bind(filter.status.as_deref())
            .bind(pending)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| ApiError::bad_request("cursor is invalid or has expired."))?;
            state
                .request_metrics
                .record_legacy_cursor(LegacyCursorEndpoint::AdminProducts);
            Ok(Some(ProductCursor {
                stable_id,
                id: legacy_id,
            }))
        }
    }
}

async fn load_facets(
    state: &AppState,
    filter: &ProductListFilter,
) -> Result<
    (
        Vec<ProductFacetCount>,
        Vec<ProductFacetCount>,
        Vec<ProductFacetCount>,
    ),
    ApiError,
> {
    let pending = filter.data_state.as_deref().map(|value| value == "pending");
    let rows = sqlx::query(
        r#"WITH matched AS (
             SELECT product.family,product.status,
                    EXISTS(SELECT 1 FROM product_specs specification
                           WHERE specification.product_id=product.id
                             AND specification.product_revision=product.current_revision
                             AND specification.fact_state='pendingVerification') AS pending
             FROM products product
             LEFT JOIN product_presentation_working presentation
               ON presentation.product_id=product.id AND presentation.locale=product.locale
             WHERE ($1::text IS NULL OR product.stable_id ILIKE '%'||$1||'%'
                    OR product.model ILIKE '%'||$1||'%'
                    OR presentation.title ILIKE '%'||$1||'%'
                    OR product.family ILIKE '%'||$1||'%')
               AND ($2::text IS NULL OR product.status=$2)
               AND ($3::boolean IS NULL OR $3 = EXISTS(
                     SELECT 1 FROM product_specs specification
                     WHERE specification.product_id=product.id
                       AND specification.product_revision=product.current_revision
                       AND specification.fact_state='pendingVerification'))
           )
           SELECT dimension,value,count(*) AS count FROM (
             SELECT 'family' AS dimension,family AS value FROM matched
             UNION ALL SELECT 'status',status FROM matched
             UNION ALL SELECT 'dataState',CASE WHEN pending THEN 'pending' ELSE 'verified' END FROM matched
           ) facets GROUP BY dimension,value ORDER BY dimension,value"#,
    )
    .bind(filter.search.as_deref())
    .bind(filter.status.as_deref())
    .bind(pending)
    .fetch_all(&state.pool)
    .await?;
    let mut family = Vec::new();
    let mut status = Vec::new();
    let mut data_state = Vec::new();
    for row in rows {
        let count = usize::try_from(row.try_get::<i64, _>("count")?).unwrap_or(usize::MAX);
        let facet = ProductFacetCount {
            value: row.try_get("value")?,
            count,
        };
        match row.try_get::<String, _>("dimension")?.as_str() {
            "family" => family.push(facet),
            "status" => status.push(facet),
            _ => data_state.push(facet),
        }
    }
    Ok((family, status, data_state))
}
