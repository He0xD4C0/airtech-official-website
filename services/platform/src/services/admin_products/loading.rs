use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{AdminProductDetail, DataClass, Product, ProductPresentation, ProductPrivatePricing},
    state::{decode_payload, AppState},
};

pub async fn load_admin_product_detail(
    state: &AppState,
    id: Uuid,
) -> Result<AdminProductDetail, ApiError> {
    let row = sqlx::query(
        "SELECT payload,data_origin,current_revision,product_import_run_id,stable_id FROM products WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Product was not found."))?;
    let mut product: Product = decode_payload(row.try_get("payload")?, "product")?;
    let origin: String = row.try_get("data_origin")?;
    let presentation = load_presentation(state, id, &product.locale).await?;
    if let Some(presentation) = &presentation {
        overlay_product_presentation(&mut product, presentation);
    }
    let import_run_id: Option<Uuid> = row.try_get("product_import_run_id")?;
    let missing_assets = if let Some(import_run_id) = import_run_id {
        sqlx::query(
            r#"SELECT source_record_id,asset_type,source_reference
               FROM product_import_missing_assets
               WHERE import_run_id=$1 AND source_record_id=$2 AND resolution_status='missing'
               ORDER BY created_at,id"#,
        )
        .bind(import_run_id)
        .bind(row.try_get::<String, _>("stable_id")?)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .map(|missing| {
            Ok(crate::models::MissingAssetReference {
                stable_id: missing.try_get("source_record_id")?,
                asset_type: missing.try_get("asset_type")?,
                source_reference: missing.try_get("source_reference")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        Vec::new()
    };
    Ok(AdminProductDetail {
        product,
        source_kind: match origin.as_str() {
            "verifiedCsv" => DataClass::VerifiedCsv,
            "developmentFixture" => DataClass::DevelopmentFixture,
            "feishu" => DataClass::Feishu,
            _ => DataClass::Editorial,
        },
        missing_assets,
        presentation,
    })
}

async fn load_presentation(
    state: &AppState,
    id: Uuid,
    locale: &str,
) -> Result<Option<ProductPresentation>, ApiError> {
    sqlx::query(
        r#"SELECT locale,slug,title,summary,content,seo_metadata,indexable,
                  current_revision,published_revision,updated_at
           FROM product_presentation_working WHERE product_id=$1
           ORDER BY CASE WHEN locale=$2 THEN 0 ELSE 1 END,locale LIMIT 1"#,
    )
    .bind(id)
    .bind(locale)
    .fetch_optional(&state.pool)
    .await?
    .map(decode_presentation)
    .transpose()
}

fn decode_presentation(row: sqlx::postgres::PgRow) -> Result<ProductPresentation, ApiError> {
    let content: Value = row.try_get("content")?;
    Ok(ProductPresentation {
        locale: row.try_get("locale")?,
        slug: row.try_get("slug")?,
        title: row.try_get("title")?,
        summary: row.try_get("summary")?,
        seo: decode_payload(row.try_get("seo_metadata")?, "product presentation SEO")?,
        indexable: row.try_get("indexable")?,
        sort_order: content
            .get("sortOrder")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or_default(),
        related_content_ids: content
            .get("relatedContentIds")
            .cloned()
            .map(|value| decode_payload(value, "product related content ids"))
            .transpose()?
            .unwrap_or_default(),
        media_gallery: content
            .get("mediaGallery")
            .cloned()
            .map(|value| decode_payload(value, "product media gallery"))
            .transpose()?
            .unwrap_or_default(),
        revision: row.try_get("current_revision")?,
        published_revision: row.try_get("published_revision")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub fn overlay_product_presentation(product: &mut Product, presentation: &ProductPresentation) {
    product.slug = presentation.slug.clone();
    product.locale = presentation.locale.clone();
    product.title = presentation.title.clone();
    product.summary = presentation.summary.clone();
    product.seo = presentation.seo.clone();
    product.indexable = presentation.indexable;
    product.sort_order = presentation.sort_order;
    product.related_content_ids = presentation.related_content_ids.clone();
    product.media_gallery = presentation.media_gallery.clone();
}

pub async fn load_private_pricing(
    state: &AppState,
    id: Uuid,
) -> Result<ProductPrivatePricing, ApiError> {
    let key = state
        .config
        .product_staging_encryption_key
        .as_ref()
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Product staging encryption is not configured; pricing cannot be decrypted.",
            )
        })?;
    let row = sqlx::query(
        r#"SELECT product.stable_id,staging.source_row_number,staging.nonce,
                  staging.ciphertext,staging.authentication_tag,
                  import_run.mapping_version,import_run.source_checksum
           FROM products product
           JOIN product_import_private_staging staging
             ON staging.import_run_id=product.product_import_run_id
            AND staging.source_record_id=product.stable_id
           JOIN product_import_runs import_run ON import_run.id=staging.import_run_id
           WHERE product.id=$1 AND product.data_origin='verifiedCsv'
             AND staging.status IN ('validated','promoted') AND staging.expires_at > now()
           ORDER BY staging.created_at DESC LIMIT 1"#,
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("Private pricing was not found for this product."))?;
    let stable_id: String = row.try_get("stable_id")?;
    let source_row_number = row.try_get("source_row_number")?;
    let nonce: Vec<u8> = row.try_get("nonce")?;
    let ciphertext: Vec<u8> = row.try_get("ciphertext")?;
    let authentication_tag: Vec<u8> = row.try_get("authentication_tag")?;
    let mapping_version: String = row.try_get("mapping_version")?;
    let checksum: String = row.try_get("source_checksum")?;
    let pricing_fields = crate::services::product_import::decrypt_private_pricing(
        key,
        crate::services::product_import::PrivatePricingEnvelope {
            mapping_version: &mapping_version,
            checksum: &checksum,
            source_row_number,
            stable_id: &stable_id,
            nonce: &nonce,
            ciphertext: &ciphertext,
            authentication_tag: &authentication_tag,
        },
    )?;
    Ok(ProductPrivatePricing {
        product_id: id,
        stable_id,
        source_row_number,
        pricing_fields,
    })
}
