async fn published_motor_technologies(
    state: &AppState,
    locale: &str,
) -> Result<Vec<String>, ApiError> {
    if let Some(pool) = &state.pool {
        return Ok(sqlx::query_scalar::<_, String>(
            r#"SELECT DISTINCT revision.payload->>'motorTechnology'
               FROM products product
               JOIN product_revisions revision ON revision.product_id=product.id
                 AND revision.revision=product.published_revision
               JOIN product_localizations localization ON localization.product_id=product.id
                 AND localization.product_revision=product.published_revision
                 AND localization.locale=$1 AND localization.translation_state='verified'
               JOIN public_routes route ON route.entity_type='product'
                 AND route.entity_id=product.id
                 AND route.locale=localization.locale
                 AND route.canonical_path=localization.seo_metadata->>'canonicalPath'
               WHERE product.published_revision IS NOT NULL
                 AND revision.payload->>'locale'=$1
                 AND nullif(trim(revision.payload->>'motorTechnology'),'') IS NOT NULL
               ORDER BY 1"#,
        )
        .bind(locale)
        .fetch_all(pool)
        .await?);
    }
    let mut values = state
        .data
        .read()
        .await
        .published_products
        .values()
        .filter(|product| product.locale == locale)
        .filter_map(|product| product.motor_technology.clone())
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    Ok(values)
}
