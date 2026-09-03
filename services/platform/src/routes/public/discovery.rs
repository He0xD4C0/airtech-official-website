async fn discovery(State(state): State<AppState>) -> Result<Json<DiscoveryDocument>, ApiError> {
    if super::public_data::published_site_shell_has_placeholder(&state, "en").await? {
        return Ok(Json(DiscoveryDocument {
            generated_at: Utc::now(),
            entries: Vec::new(),
        }));
    }
    if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT route.entity_type,route.entity_id,route.canonical_path,route.locale,
                      COALESCE(content_revision.payload->>'title',product_localization.title) AS title,
                      COALESCE(content_revision.payload->>'summary',product_localization.summary) AS summary,
                      COALESCE(content_revision.created_at,product_revision.created_at,route.updated_at) AS updated_at
               FROM public_routes route
               LEFT JOIN content_entries content_entry
                 ON route.entity_type='content' AND content_entry.id=route.entity_id
                AND content_entry.published_revision IS NOT NULL
               LEFT JOIN content_revisions content_revision
                 ON content_revision.content_id=content_entry.id
                AND content_revision.revision=content_entry.published_revision
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
                       AND COALESCE((content_revision.payload->>'isPlaceholder')::boolean,false)=false)
                   OR (route.entity_type='product' AND product.id IS NOT NULL
                       AND product_localization.translation_state='verified'))
               ORDER BY route.canonical_path"#,
        )
        .fetch_all(pool)
        .await?;
        let entries = rows
            .into_iter()
            .map(|row| {
                Ok(DiscoveryEntry {
                    entity_type: match row.try_get::<String, _>("entity_type")?.as_str() {
                        "product" => "product",
                        _ => "content",
                    },
                    entity_id: row.try_get("entity_id")?,
                    path: row.try_get("canonical_path")?,
                    locale: row.try_get("locale")?,
                    title: row.try_get("title")?,
                    summary: row.try_get("summary")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        return Ok(Json(DiscoveryDocument {
            generated_at: Utc::now(),
            entries,
        }));
    }
    let (content_values, product_values) = if let Some(pool) = &state.pool {
        (
            load_published_content_rows(pool, None, None, None).await?,
            load_published_product_rows(pool, None, None, None).await?,
        )
    } else {
        let data = state.data.read().await;
        (
            data.published_content.values().cloned().collect(),
            data.published_products.values().cloned().collect(),
        )
    };
    let mut entries = Vec::new();
    entries.extend(content_values.iter().filter_map(|content| {
        let path = content.seo.canonical_path.as_ref()?;
        if content.is_placeholder || !content.seo.indexable || !valid_public_path(path) {
            return None;
        }
        Some(DiscoveryEntry {
            entity_type: "content",
            entity_id: content.id,
            path: path.clone(),
            locale: content.locale.clone(),
            title: content.title.clone(),
            summary: content.summary.clone(),
            updated_at: content.updated_at,
        })
    }));
    entries.extend(product_values.iter().filter_map(|product| {
        if !product.indexable || product.locale != "en" || !valid_slug_segment(&product.slug) {
            return None;
        }
        Some(DiscoveryEntry {
            entity_type: "product",
            entity_id: product.id,
            path: format!(
                "/en/products/{}/{}",
                product_family_segment(product.family),
                product.slug
            ),
            locale: product.locale.clone(),
            title: product.title.clone(),
            summary: product.summary.clone(),
            updated_at: product.updated_at,
        })
    }));
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Json(DiscoveryDocument {
        generated_at: Utc::now(),
        entries,
    }))
}

fn valid_public_path(path: &str) -> bool {
    (path == "/en" || path.starts_with("/en/"))
        && !path.contains('?')
        && !path.contains('#')
        && !path.contains("//")
        && !path.starts_with("/en/admin")
}

fn valid_slug_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn product_family_segment(value: crate::models::ProductFamily) -> &'static str {
    match value {
        crate::models::ProductFamily::Centrifugal => "centrifugal",
        crate::models::ProductFamily::Axial => "axial",
        crate::models::ProductFamily::CrossFlow => "cross-flow",
        crate::models::ProductFamily::InlineDuct => "inline-duct",
        crate::models::ProductFamily::Motors => "motors",
    }
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
