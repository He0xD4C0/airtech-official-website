async fn resolve_route(
    State(state): State<AppState>,
    Query(query): Query<ResolveRouteQuery>,
) -> Result<Json<RouteResolution>, ApiError> {
    validate_locale(&query.locale)?;
    validate_public_path(&query.path)?;

    let page = published_content_by_path(&state, &query.path, &query.locale).await?;
    if let Some(page) = page {
        let data_class = if page.is_placeholder {
            DataClass::DevelopmentFixture
        } else {
            DataClass::Editorial
        };
        let template_key =
            page_template_key(&page).unwrap_or_else(|| template_key(page.kind).to_owned());
        let indexable = page.seo.indexable && !page.is_placeholder;
        return Ok(Json(RouteResolution {
            path: query.path,
            template_key,
            entity_type: "content".into(),
            entity_id: Some(page.id),
            locale: query.locale,
            published_revision: page.published_revision,
            indexable,
            data_class,
            page: Some(page),
        }));
    }

    if let Some(pool) = &state.pool {
        let route = sqlx::query(
            r#"SELECT route.entity_type,route.entity_id,route.indexable,
                      product.published_revision,product.data_origin
               FROM public_routes route
               JOIN products product
                 ON route.entity_type='product' AND product.id=route.entity_id
                AND product.published_revision IS NOT NULL
               JOIN product_localizations localization
                 ON localization.product_id=product.id
                AND localization.product_revision=product.published_revision
                AND localization.locale=route.locale
                AND localization.translation_state='verified'
               WHERE route.canonical_path=$1 AND route.locale=$2"#,
        )
        .bind(&query.path)
        .bind(&query.locale)
        .fetch_optional(pool)
        .await?;
        if let Some(row) = route {
            let entity_type: String = row.try_get("entity_type")?;
            return Ok(Json(RouteResolution {
                path: query.path,
                template_key: if entity_type == "product" {
                    "productDetail".into()
                } else {
                    entity_type.clone()
                },
                entity_type,
                entity_id: Some(row.try_get("entity_id")?),
                locale: query.locale,
                published_revision: row.try_get("published_revision")?,
                indexable: row.try_get("indexable")?,
                data_class: decode_data_class(row.try_get("data_origin")?),
                page: None,
            }));
        }
    }
    Err(ApiError::not_found("Published route was not found."))
}
