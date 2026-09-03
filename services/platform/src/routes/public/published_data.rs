fn with_etag<T: serde::Serialize>(value: T, revision: i64) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response
}

async fn load_published_content_rows(
    pool: &sqlx::PgPool,
    kind: Option<ContentKind>,
    slug: Option<&str>,
    locale: Option<&str>,
) -> Result<Vec<crate::models::ContentEntry>, ApiError> {
    let kind = kind.map(|value| {
        serde_json::to_value(value)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default()
    });
    let rows = sqlx::query(
        r#"SELECT payload,published_revision FROM published_content
           WHERE ($1::text IS NULL OR kind=$1)
             AND ($2::text IS NULL OR slug=$2)
             AND ($3::text IS NULL OR locale=$3)
           ORDER BY updated_at DESC,id"#,
    )
    .bind(kind)
    .bind(slug)
    .bind(locale)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            let mut entry: crate::models::ContentEntry =
                serde_json::from_value(row.try_get("payload")?).map_err(|error| {
                    tracing::error!(%error, "published content payload is invalid");
                    ApiError::service_unavailable("Stored published content is invalid.")
                })?;
            let revision: i64 = row.try_get("published_revision")?;
            entry.status = crate::models::PublicationStatus::Published;
            entry.current_revision = revision;
            entry.published_revision = Some(revision);
            if entry.is_placeholder {
                entry.seo.indexable = false;
            }
            Ok(entry)
        })
        .collect()
}

async fn load_published_product_rows(
    pool: &sqlx::PgPool,
    family: Option<ProductFamily>,
    motor_technology: Option<&str>,
    slug: Option<&str>,
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
           ORDER BY published.stable_id,published.id"#,
    )
    .bind(family)
    .bind(motor_technology)
    .bind(slug)
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
            // The immutable published presentation controls public indexing.
            // A later working-draft edit must not hide the prior projection.
            product.indexable = row.try_get::<bool, _>("localized_indexable")?;
            Ok(product)
        })
        .collect()
}

fn parse_content_kind(value: &str) -> Result<ContentKind, ApiError> {
    match value {
        "home" => Ok(ContentKind::Home),
        "solutions" => Ok(ContentKind::Solution),
        "technology" => Ok(ContentKind::Technology),
        "articles" => Ok(ContentKind::Article),
        "news" => Ok(ContentKind::News),
        "faqs" => Ok(ContentKind::Faq),
        "case-studies" => Ok(ContentKind::CaseStudy),
        "downloads" => Ok(ContentKind::Download),
        "company" => Ok(ContentKind::Company),
        "legal" => Ok(ContentKind::Legal),
        _ => Err(ApiError::not_found("Content kind was not found.")),
    }
}
