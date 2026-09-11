async fn discovery(State(state): State<AppState>) -> Result<Json<DiscoveryDocument>, ApiError> {
    if super::public_data::published_site_shell_has_placeholder(&state, "en").await? {
        return Ok(Json(DiscoveryDocument {
            generated_at: Utc::now(),
            entries: Vec::new(),
        }));
    }
    let pool = state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("The public projection store is unavailable.")
    })?;
    let rows = sqlx::query(
        r#"SELECT route.entity_type,route.entity_id,route.canonical_path,route.locale,
                  content_entry.template_key AS content_template_key,
                  content_revision.document->>'slug' AS content_slug,
                  CASE route.entity_type
                    WHEN 'content' THEN content_revision.document->>'title'
                    WHEN 'product' THEN product_localization.title
                  END AS title,
                  CASE route.entity_type
                    WHEN 'content' THEN content_revision.document->>'summary'
                    WHEN 'product' THEN product_localization.summary
                  END AS summary,
                  CASE route.entity_type
                    WHEN 'content' THEN content_revision.created_at
                    WHEN 'product' THEN product_revision.created_at
                    ELSE route.updated_at
                  END AS updated_at
           FROM public_routes route
           LEFT JOIN content_entries content_entry
             ON route.entity_type='content' AND content_entry.id=route.entity_id
            AND content_entry.cms_published_revision IS NOT NULL
           LEFT JOIN content_revisions content_revision
             ON content_revision.content_id=content_entry.id
            AND content_revision.revision=content_entry.cms_published_revision
            AND content_revision.document IS NOT NULL
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
             AND ((route.entity_type='content'
                   AND content_entry.id IS NOT NULL
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
            let template_key = template_key
                .and_then(|value| serde_json::from_value(serde_json::Value::String(value)).ok());
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
    Ok(Json(DiscoveryDocument {
        generated_at: Utc::now(),
        entries,
    }))
}

#[derive(Debug, Deserialize)]
struct ContentQuery {
    #[serde(default = "default_locale")]
    locale: String,
}

#[derive(Debug, Deserialize)]
struct ProductDetailQuery {
    family: Option<ProductFamily>,
}
