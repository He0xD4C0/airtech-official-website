use super::*;

pub struct ProductRouteProjection {
    pub entity_type: String,
    pub entity_id: Uuid,
    pub published_revision: Option<i64>,
    pub indexable: bool,
    pub data_class: DataClass,
}

pub async fn load_product_route(
    pool: &sqlx::PgPool,
    path: &str,
    locale: &str,
) -> Result<Option<ProductRouteProjection>, ApiError> {
    let row = sqlx::query(
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
    .bind(path)
    .bind(locale)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        let data_origin: String = row.try_get("data_origin")?;
        Ok(ProductRouteProjection {
            entity_type: row.try_get("entity_type")?,
            entity_id: row.try_get("entity_id")?,
            published_revision: row.try_get("published_revision")?,
            indexable: row.try_get("indexable")?,
            data_class: match data_origin.as_str() {
                "developmentFixture" => DataClass::DevelopmentFixture,
                "feishu" => DataClass::Feishu,
                "verifiedCsv" => DataClass::VerifiedCsv,
                _ => DataClass::Editorial,
            },
        })
    })
    .transpose()
}
