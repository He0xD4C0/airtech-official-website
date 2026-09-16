use super::*;

pub(super) async fn resolve_route(
    State(state): State<AppState>,
    Query(query): Query<ResolveRouteQuery>,
) -> Result<Json<RouteResolution>, ApiError> {
    validate_locale(&query.locale)?;
    validate_public_path(&query.path)?;

    if let Some(route) = load_v2_route(&state, &query.path, &query.locale).await? {
        let placeholder = route.page.is_placeholder;
        return Ok(Json(RouteResolution {
            path: query.path,
            template_key: route.template_key,
            entity_type: "content".into(),
            entity_id: Some(route.entity_id),
            locale: query.locale,
            published_revision: Some(route.revision),
            indexable: route.indexable && !placeholder,
            data_class: if placeholder {
                DataClass::DevelopmentFixture
            } else {
                DataClass::Editorial
            },
            page: Some(route.page),
        }));
    }

    let route = crate::services::public_content::load_product_route(
        &state.pool,
        &query.path,
        &query.locale,
    )
    .await?;
    if let Some(row) = route {
        let entity_type = row.entity_type;
        return Ok(Json(RouteResolution {
            path: query.path,
            template_key: if entity_type == "product" {
                "productDetail".into()
            } else {
                entity_type.clone()
            },
            entity_type,
            entity_id: Some(row.entity_id),
            locale: query.locale,
            published_revision: row.published_revision,
            indexable: row.indexable,
            data_class: row.data_class,
            page: None,
        }));
    }
    Err(ApiError::not_found("Published route was not found."))
}
