use super::*;
use crate::models::ProductFacetCount;

#[derive(Default)]
pub(crate) struct PublishedProductFilter<'a> {
    pub(crate) family: Option<ProductFamily>,
    pub(crate) motor_technology: Option<&'a str>,
    pub(crate) search: Option<&'a str>,
    pub(crate) slug: Option<&'a str>,
    pub(crate) product_id: Option<Uuid>,
    pub(crate) after: Option<(&'a str, Uuid)>,
    pub(crate) limit: Option<usize>,
}

pub(crate) async fn load_published_product_rows<
    'e,
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
>(
    pool: E,
    filter: PublishedProductFilter<'_>,
) -> Result<Vec<Product>, ApiError> {
    let family = filter.family.map(family_label);
    let rows = sqlx::query(&format!(
        r#"SELECT published.payload,published.published_revision,
                  localization.slug AS localized_slug,localization.title AS localized_title,
                  localization.summary AS localized_summary,
                  localization.content AS localized_content,
                  localization.seo_metadata AS localized_seo,
                  localization.indexable AS localized_indexable
           {PRODUCT_MATCHES_SQL}
             AND ($1::text IS NULL OR published.family=$1)
             AND ($2::text IS NULL OR nullif(trim(published.payload->>'motorTechnology'),'')=$2)
             AND ($4::text IS NULL OR localization.slug=$4)
             AND ($5::uuid IS NULL OR published.id=$5)
             AND ($6::text IS NULL OR (published.stable_id,published.id)>($6,$7))
           ORDER BY published.stable_id,published.id LIMIT $8"#
    ))
    .bind(family)
    .bind(filter.motor_technology)
    .bind(filter.search)
    .bind(filter.slug)
    .bind(filter.product_id)
    .bind(filter.after.map(|value| value.0))
    .bind(filter.after.map(|value| value.1))
    .bind(i64::try_from(filter.limit.unwrap_or(usize::MAX)).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let mut product: Product =
                serde_json::from_value(row.try_get("payload")?).map_err(|error| {
                    tracing::error!(%error, "published product payload is invalid");
                    ApiError::service_unavailable("Stored published product is invalid.")
                })?;
            let revision: i64 = row.try_get("published_revision")?;
            product.status = crate::models::PublicationStatus::Published;
            product.current_revision = revision;
            product.published_revision = Some(revision);
            product.slug = row.try_get("localized_slug")?;
            product.title = row.try_get("localized_title")?;
            product.summary = row.try_get("localized_summary")?;
            let content: Value = row.try_get("localized_content")?;
            product.seo =
                serde_json::from_value(row.try_get("localized_seo")?).map_err(|error| {
                    tracing::error!(%error, "published product SEO is invalid");
                    ApiError::service_unavailable("Stored published product SEO is invalid.")
                })?;
            product.sort_order = content
                .get("sortOrder")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or_default();
            product.related_content_ids = content
                .get("relatedContentIds")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| {
                    tracing::error!(%error, "published product related content is invalid");
                    ApiError::service_unavailable(
                        "Stored published product related content is invalid.",
                    )
                })?
                .unwrap_or_default();
            product.media_gallery = content
                .get("mediaGallery")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| {
                    tracing::error!(%error, "published product media gallery is invalid");
                    ApiError::service_unavailable(
                        "Stored published product media gallery is invalid.",
                    )
                })?
                .unwrap_or_default();
            product.indexable = row.try_get::<bool, _>("localized_indexable")?;
            Ok(product)
        })
        .collect()
}

pub(crate) async fn load_published_product_facets<
    'e,
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
>(
    pool: E,
    family: Option<ProductFamily>,
    motor_technology: Option<&str>,
    search: Option<&str>,
) -> Result<(usize, Vec<ProductFacetCount>, Vec<ProductFacetCount>), ApiError> {
    let rows = sqlx::query(&format!(
        r#"WITH matched AS (
             SELECT published.family,
                    nullif(trim(published.payload->>'motorTechnology'),'') AS motor_technology
             {PRODUCT_MATCHES_SQL}
           ), facets AS (
             SELECT 'total' AS dimension,'' AS value,count(*) AS count
             FROM matched
             WHERE ($1::text IS NULL OR family=$1)
               AND ($2::text IS NULL OR motor_technology=$2)
             UNION ALL
             SELECT 'family',family,count(*) FROM matched
             WHERE ($2::text IS NULL OR motor_technology=$2)
             GROUP BY family
             UNION ALL
             SELECT 'motorTechnology',motor_technology,count(*) FROM matched
             WHERE motor_technology IS NOT NULL AND ($1::text IS NULL OR family=$1)
             GROUP BY motor_technology
           )
           SELECT dimension,value,count FROM facets ORDER BY dimension,value"#
    ))
    .bind(family.map(family_label))
    .bind(motor_technology)
    .bind(search)
    .fetch_all(pool)
    .await?;
    let mut total = 0;
    let mut families = Vec::new();
    let mut motor_technologies = Vec::new();
    for row in rows {
        let count = usize::try_from(row.try_get::<i64, _>("count")?).unwrap_or(usize::MAX);
        match row.try_get::<String, _>("dimension")?.as_str() {
            "total" => total = count,
            "family" => families.push(ProductFacetCount {
                value: row.try_get("value")?,
                count,
            }),
            _ => motor_technologies.push(ProductFacetCount {
                value: row.try_get("value")?,
                count,
            }),
        }
    }
    Ok((total, families, motor_technologies))
}

fn family_label(value: ProductFamily) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) async fn published_motor_technologies(
    pool: &sqlx::PgPool,
    locale: &str,
) -> Result<Vec<String>, ApiError> {
    Ok(sqlx::query_scalar::<_, String>(
        r#"SELECT DISTINCT revision.payload->>'motorTechnology'
           FROM products product
           JOIN product_revisions revision ON revision.product_id=product.id
             AND revision.revision=product.published_revision
           JOIN product_localizations localization ON localization.product_id=product.id
             AND localization.product_revision=product.published_revision
             AND localization.locale=$1 AND localization.translation_state='verified'
           JOIN public_routes route ON route.entity_type='product'
             AND route.entity_id=product.id AND route.locale=localization.locale
             AND route.canonical_path=localization.seo_metadata->>'canonicalPath'
           WHERE product.published_revision IS NOT NULL
             AND revision.payload->>'locale'=$1
             AND nullif(trim(revision.payload->>'motorTechnology'),'') IS NOT NULL
           ORDER BY 1"#,
    )
    .bind(locale)
    .fetch_all(pool)
    .await?)
}

pub(crate) async fn load_published_product_assets(
    pool: &sqlx::PgPool,
    slug: &str,
    family: Option<ProductFamily>,
) -> Result<ProductSourceAssetDocument, ApiError> {
    let family = family.map(family_label);
    let products = sqlx::query(
        r#"SELECT published.id,published.published_revision
           FROM published_products published
           JOIN product_localizations localization
             ON localization.product_id=published.id
            AND localization.product_revision=published.published_revision
            AND localization.locale=published.locale
           JOIN public_routes route
             ON route.entity_type='product' AND route.entity_id=published.id
            AND route.locale=published.locale
            AND route.canonical_path=localization.seo_metadata->>'canonicalPath'
           WHERE localization.translation_state='verified' AND localization.slug=$1
             AND ($2::text IS NULL OR published.family=$2)
           ORDER BY published.id LIMIT 2"#,
    )
    .bind(slug)
    .bind(family)
    .fetch_all(pool)
    .await?;
    let product = match products.as_slice() {
        [] => return Err(ApiError::not_found("Published product was not found.")),
        [product] => product,
        _ => {
            return Err(ApiError::conflict(
                "The product slug is shared by more than one family; include the family query parameter.",
            ))
        }
    };
    let product_id: Uuid = product.try_get("id")?;
    let product_revision: i64 = product.try_get("published_revision")?;
    let expected: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM asset_references WHERE product_id=$1 AND product_revision=$2",
    )
    .bind(product_id)
    .bind(product_revision)
    .fetch_one(pool)
    .await?;
    let rows = sqlx::query(
        r#"SELECT reference.usage,asset.id,asset.original_name,asset.media_type,
                  asset.byte_size,asset.checksum,asset.preview_storage_key
           FROM asset_references reference
           JOIN media_assets asset ON asset.id=reference.media_asset_id
             AND asset.deleted_at IS NULL
           WHERE reference.product_id=$1 AND reference.product_revision=$2
           ORDER BY reference.sort_order,reference.id"#,
    )
    .bind(product_id)
    .bind(product_revision)
    .fetch_all(pool)
    .await?;
    if expected != rows.len() as i64 {
        return Err(ApiError::service_unavailable(
            "Published product attachments are temporarily incomplete.",
        ));
    }
    let items = rows
        .iter()
        .map(|row| {
            let asset_id: Uuid = row.try_get("id")?;
            let has_preview = row
                .try_get::<Option<String>, _>("preview_storage_key")?
                .is_some();
            Ok(ProductSourceAsset {
                asset_id,
                usage: row.try_get("usage")?,
                original_name: row.try_get("original_name")?,
                media_type: row.try_get("media_type")?,
                byte_size: row.try_get("byte_size")?,
                sha256: row.try_get("checksum")?,
                download_url: format!("/api/public/v1/media/{asset_id}/download"),
                preview_url: has_preview.then(|| format!("/api/public/v1/media/{asset_id}")),
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(ProductSourceAssetDocument {
        product_id,
        product_revision,
        items,
    })
}

const PRODUCT_MATCHES_SQL: &str = r#"FROM published_products published
           JOIN product_localizations localization
             ON localization.product_id=published.id
            AND localization.product_revision=published.published_revision
            AND localization.locale=published.locale
           JOIN public_routes route
             ON route.entity_type='product'
            AND route.entity_id=published.id
            AND route.locale=published.locale
            AND route.canonical_path=localization.seo_metadata->>'canonicalPath'
           WHERE localization.translation_state='verified' AND published.locale='en'
             AND ($3::text IS NULL OR NOT EXISTS (
                   SELECT 1 FROM unnest(string_to_array($3,' ')) AS search_term
                   WHERE NOT (
                     strpos(lower(localization.title),search_term)>0
                     OR strpos(lower(published.stable_id),search_term)>0
                     OR strpos(lower(coalesce(published.payload->>'model','')),search_term)>0
                     OR strpos(lower(coalesce(published.payload->>'subtype','')),search_term)>0
                     OR EXISTS (
                       SELECT 1
                       FROM jsonb_array_elements(coalesce(published.payload->'specifications','[]'::jsonb)) specification
                       WHERE specification->>'state'='verified'
                         AND jsonb_typeof(specification->'value') IN ('string','number','boolean')
                         AND strpos(lower(specification->>'value'),search_term)>0
                     )
                   )
                 ))
"#;

pub(crate) async fn load_catalog_page(
    pool: &sqlx::PgPool,
    filter: PublishedProductFilter<'_>,
) -> Result<
    (
        Vec<Product>,
        (usize, Vec<ProductFacetCount>, Vec<ProductFacetCount>),
    ),
    ApiError,
> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let facets = load_published_product_facets(
        &mut *transaction,
        filter.family,
        filter.motor_technology,
        filter.search,
    )
    .await?;
    let products = load_published_product_rows(&mut *transaction, filter).await?;
    transaction.commit().await?;
    Ok((products, facets))
}
