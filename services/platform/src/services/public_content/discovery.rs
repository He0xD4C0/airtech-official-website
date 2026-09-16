use super::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoveryEntry {
    pub(crate) entity_type: &'static str,
    pub(crate) entity_id: Uuid,
    pub(crate) path: String,
    pub(crate) locale: String,
    pub(crate) title: String,
    pub(crate) summary: Option<String>,
    pub(crate) updated_at: DateTime<Utc>,
}

pub(crate) async fn load_discovery_entries(
    pool: &sqlx::PgPool,
) -> Result<Vec<DiscoveryEntry>, ApiError> {
    let rows = sqlx::query(
        r#"SELECT route.entity_type,route.entity_id,route.canonical_path,route.locale,
                  content_entry.template_key AS content_template_key,
                  content_revision.document->>'slug' AS content_slug,
                  CASE route.entity_type
                    WHEN 'content' THEN content_revision.document->>'title'
                    WHEN 'product' THEN product_localization.title END AS title,
                  CASE route.entity_type
                    WHEN 'content' THEN content_revision.document->>'summary'
                    WHEN 'product' THEN product_localization.summary END AS summary,
                  CASE route.entity_type
                    WHEN 'content' THEN content_revision.updated_at
                    WHEN 'product' THEN product_revision.created_at
                    ELSE route.updated_at END AS updated_at
           FROM public_routes route
           LEFT JOIN content_entries content_entry
             ON route.entity_type='content' AND content_entry.id=route.entity_id
           LEFT JOIN cms_published_content content_revision
             ON content_revision.content_id=content_entry.id
           LEFT JOIN products product
             ON route.entity_type='product' AND product.id=route.entity_id
            AND product.published_revision IS NOT NULL
           LEFT JOIN product_revisions product_revision
             ON product_revision.product_id=product.id
            AND product_revision.revision=product.published_revision
           LEFT JOIN product_localizations product_localization
             ON product_localization.product_id=product.id
            AND product_localization.product_revision=product.published_revision
            AND product_localization.locale=route.locale
           WHERE route.indexable=true
             AND ((route.entity_type='content' AND content_entry.id IS NOT NULL
                   AND content_revision.document->>'schemaVersion'='2'
                   AND content_revision.document->>'locale'=route.locale
                   AND content_revision.document->>'kind'=content_entry.kind
                   AND content_revision.document->>'templateKey'=content_entry.template_key
                   AND content_revision.document->>'isPlaceholder'='false')
               OR (route.entity_type='product' AND product.id IS NOT NULL
                   AND product_localization.translation_state='verified'
                   AND route.canonical_path=product_localization.seo_metadata->>'canonicalPath'))
           ORDER BY route.canonical_path"#,
    )
    .fetch_all(pool)
    .await?;
    let mut entries = Vec::new();
    for row in rows {
        let entity_type: String = row.try_get("entity_type")?;
        let path: String = row.try_get("canonical_path")?;
        let locale: String = row.try_get("locale")?;
        if entity_type == "content" {
            let template_key: Option<String> = row.try_get("content_template_key")?;
            let template_key =
                template_key.and_then(|value| serde_json::from_value(Value::String(value)).ok());
            let slug: Option<String> = row.try_get("content_slug")?;
            let canonical = template_key.and_then(|template_key| {
                crate::services::cms_templates::canonical_path(
                    &locale,
                    template_key,
                    slug.as_deref(),
                )
            });
            if canonical.as_deref() != Some(path.as_str()) {
                continue;
            }
        }
        entries.push(DiscoveryEntry {
            entity_type: if entity_type == "product" {
                "product"
            } else {
                "content"
            },
            entity_id: row.try_get("entity_id")?,
            path,
            locale,
            title: row.try_get("title")?,
            summary: row.try_get("summary")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(entries)
}
