use super::*;

pub(crate) async fn load_published_product_rows(
    pool: &sqlx::PgPool,
    family: Option<ProductFamily>,
    motor_technology: Option<&str>,
    slug: Option<&str>,
    product_id: Option<Uuid>,
    after: Option<(&str, Uuid)>,
    limit: Option<usize>,
) -> Result<Vec<Product>, ApiError> {
    let family = family.map(|value| {
        serde_json::to_value(value)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    });
    let rows = sqlx::query(
        r#"SELECT published.payload,published.published_revision,
                  localization.slug AS localized_slug,localization.title AS localized_title,
                  localization.summary AS localized_summary,
                  localization.content AS localized_content,
                  localization.seo_metadata AS localized_seo,
                  localization.indexable AS localized_indexable
           FROM published_products published
           JOIN product_localizations localization
             ON localization.product_id=published.id
            AND localization.product_revision=published.published_revision
            AND localization.locale=published.locale
           JOIN public_routes route
             ON route.entity_type='product'
            AND route.entity_id=published.id
            AND route.locale=published.locale
            AND route.canonical_path=localization.seo_metadata->>'canonicalPath'
           WHERE localization.translation_state='verified'
             AND ($1::text IS NULL OR published.family=$1)
             AND ($2::text IS NULL OR published.payload->>'motorTechnology'=$2)
             AND ($3::text IS NULL OR localization.slug=$3)
             AND ($4::uuid IS NULL OR published.id=$4)
             AND ($5::text IS NULL OR (published.stable_id,published.id)>($5,$6))
           ORDER BY published.stable_id,published.id LIMIT $7"#,
    )
    .bind(family)
    .bind(motor_technology)
    .bind(slug)
    .bind(product_id)
    .bind(after.map(|value| value.0))
    .bind(after.map(|value| value.1))
    .bind(i64::try_from(limit.unwrap_or(usize::MAX)).unwrap_or(i64::MAX))
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
    let family = family.map(|value| {
        serde_json::to_value(value)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    });
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
             AND asset.deleted_at IS NULL AND asset.scan_status='clean'
             AND asset.access_level='public'
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
