use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{ProductPresentation, UpdateProductPresentation},
};

pub struct PresentationWriteContext {
    data_origin: String,
    exists: bool,
    published_revision: Option<i64>,
}

pub async fn lock_product_presentation(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    product_id: Uuid,
    source_revision: i64,
    expected: i64,
    input: &UpdateProductPresentation,
) -> Result<PresentationWriteContext, ApiError> {
    let product = sqlx::query(
        "SELECT current_revision,data_origin,family FROM products WHERE id=$1 FOR UPDATE",
    )
    .bind(product_id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Product was not found."))?;
    if product.try_get::<i64, _>("current_revision")? != source_revision {
        return Err(ApiError::conflict(
            "The Product Master facts changed; reload before saving presentation fields.",
        ));
    }
    let existing = sqlx::query(
        r#"SELECT current_revision,published_revision FROM product_presentation_working
           WHERE product_id=$1 AND locale=$2 FOR UPDATE"#,
    )
    .bind(product_id)
    .bind(&input.locale)
    .fetch_optional(&mut **transaction)
    .await?;
    let stored_revision = existing
        .as_ref()
        .map(|row| row.try_get::<i64, _>("current_revision"))
        .transpose()?
        .unwrap_or_default();
    if stored_revision != expected {
        return Err(ApiError::conflict(
            "The product presentation changed; reload before saving.",
        ));
    }
    let family: String = product.try_get("family")?;
    let route_key = format!(
        "product-presentation:{}:{}:{}",
        family, input.locale, input.slug
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(route_key)
        .execute(&mut **transaction)
        .await?;
    let slug_owner = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT presentation.product_id FROM product_presentation_working presentation
           JOIN products other_product ON other_product.id=presentation.product_id
           WHERE presentation.locale=$1 AND presentation.slug=$2
             AND presentation.product_id<>$3 AND other_product.family=$4"#,
    )
    .bind(&input.locale)
    .bind(&input.slug)
    .bind(product_id)
    .bind(&family)
    .fetch_optional(&mut **transaction)
    .await?;
    if slug_owner.is_some() {
        return Err(ApiError::conflict(
            "Another product in this family already uses this locale and slug.",
        ));
    }
    Ok(PresentationWriteContext {
        data_origin: product.try_get("data_origin")?,
        exists: existing.is_some(),
        published_revision: existing
            .as_ref()
            .map(|row| row.try_get("published_revision"))
            .transpose()?
            .flatten(),
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn write_product_presentation(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    product_id: Uuid,
    source_revision: i64,
    expected: i64,
    input: &UpdateProductPresentation,
    actor: &str,
    context: PresentationWriteContext,
    now: DateTime<Utc>,
) -> Result<ProductPresentation, ApiError> {
    validate_related_content(transaction, &input.related_content_ids).await?;
    let revision = expected + 1;
    let content = json!({
        "sortOrder": input.sort_order,
        "relatedContentIds": input.related_content_ids,
    });
    let seo = serde_json::to_value(&input.seo)
        .map_err(|_| ApiError::internal("Product presentation SEO serialization failed."))?;
    let is_fixture = context.data_origin == "developmentFixture";
    let indexable = input.indexable && !is_fixture;
    let data_origin = if is_fixture {
        "developmentFixture"
    } else {
        "editorial"
    };
    if context.exists {
        let updated = sqlx::query(
            r#"UPDATE product_presentation_working
               SET current_revision=$3,slug=$4,title=$5,summary=$6,content=$7,
                   seo_metadata=$8,translation_state='draft',is_placeholder=$9,
                   indexable=$10,data_origin=$11,updated_by=$12,updated_at=$13
               WHERE product_id=$1 AND locale=$2 AND current_revision=$14"#,
        )
        .bind(product_id)
        .bind(&input.locale)
        .bind(revision)
        .bind(&input.slug)
        .bind(&input.title)
        .bind(&input.summary)
        .bind(&content)
        .bind(&seo)
        .bind(is_fixture)
        .bind(indexable)
        .bind(data_origin)
        .bind(actor)
        .bind(now)
        .bind(expected)
        .execute(&mut **transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "The product presentation changed; reload before saving.",
            ));
        }
    } else {
        sqlx::query(
            r#"INSERT INTO product_presentation_working
               (product_id,locale,current_revision,published_revision,slug,title,summary,
                content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                updated_by,updated_at)
               VALUES ($1,$2,$3,NULL,$4,$5,$6,$7,$8,'draft',$9,$10,$11,$12,$13)"#,
        )
        .bind(product_id)
        .bind(&input.locale)
        .bind(revision)
        .bind(&input.slug)
        .bind(&input.title)
        .bind(&input.summary)
        .bind(&content)
        .bind(&seo)
        .bind(is_fixture)
        .bind(indexable)
        .bind(data_origin)
        .bind(actor)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }
    insert_presentation_revision(
        transaction,
        product_id,
        source_revision,
        revision,
        input,
        actor,
        &content,
        &seo,
        is_fixture,
        indexable,
        data_origin,
        now,
    )
    .await?;
    Ok(ProductPresentation {
        locale: input.locale.clone(),
        slug: input.slug.clone(),
        title: input.title.clone(),
        summary: input.summary.clone(),
        seo: input.seo.clone(),
        indexable,
        sort_order: input.sort_order,
        related_content_ids: input.related_content_ids.clone(),
        revision,
        published_revision: context.published_revision,
        updated_at: now,
    })
}

async fn validate_related_content(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ids: &[Uuid],
) -> Result<(), ApiError> {
    if ids.is_empty() {
        return Ok(());
    }
    let count =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM content_entries WHERE id=ANY($1)")
            .bind(ids)
            .fetch_one(&mut **transaction)
            .await?;
    if count != ids.len() as i64 {
        return Err(ApiError::validation(BTreeMap::from([(
            "relatedContentIds".into(),
            vec!["Every related content id must exist.".into()],
        )])));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_presentation_revision(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    product_id: Uuid,
    source_revision: i64,
    revision: i64,
    input: &UpdateProductPresentation,
    actor: &str,
    content: &serde_json::Value,
    seo: &serde_json::Value,
    is_fixture: bool,
    indexable: bool,
    data_origin: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO product_presentation_revisions
           (product_id,locale,revision,source_product_revision,slug,title,summary,
            content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
            created_by,created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'draft',$10,$11,$12,$13,$14)"#,
    )
    .bind(product_id)
    .bind(&input.locale)
    .bind(revision)
    .bind(source_revision)
    .bind(&input.slug)
    .bind(&input.title)
    .bind(&input.summary)
    .bind(content)
    .bind(seo)
    .bind(is_fixture)
    .bind(indexable)
    .bind(data_origin)
    .bind(actor)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
