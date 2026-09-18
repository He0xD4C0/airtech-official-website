use chrono::Utc;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, models::FeishuRollbackReport, state::AppState};

pub async fn rollback_sync_run(
    state: &AppState,
    sync_run_id: Uuid,
    actor: &str,
) -> Result<FeishuRollbackReport, ApiError> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *transaction)
        .await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sync_runs WHERE id=$1 AND status IN ('completed','completedWithErrors'))",
    )
    .bind(sync_run_id)
    .fetch_one(&mut *transaction)
    .await?;
    if !exists {
        return Err(ApiError::conflict(
            "Only a completed Feishu sync run can be rolled back.",
        ));
    }
    let changes = sqlx::query(
        r#"SELECT product_id,before_revision,after_revision,change_kind
           FROM feishu_sync_changes WHERE sync_run_id=$1 AND rolled_back_at IS NULL
           ORDER BY product_id FOR UPDATE"#,
    )
    .bind(sync_run_id)
    .fetch_all(&mut *transaction)
    .await?;
    if changes.is_empty() {
        return Err(ApiError::conflict(
            "This Feishu sync run has no rollback-eligible changes.",
        ));
    }
    let mut restored = 0_u64;
    let mut skipped = 0_u64;
    let now = Utc::now();
    for change in changes {
        let product_id: Uuid = change.try_get("product_id")?;
        let before_revision: Option<i64> = change.try_get("before_revision")?;
        let after_revision: i64 = change.try_get("after_revision")?;
        let product =
            sqlx::query("SELECT current_revision,payload FROM products WHERE id=$1 FOR UPDATE")
                .bind(product_id)
                .fetch_optional(&mut *transaction)
                .await?;
        let Some(product) = product else {
            skipped += 1;
            continue;
        };
        if product.try_get::<i64, _>("current_revision")? != after_revision {
            skipped += 1;
            continue;
        }
        let mut payload: Value = product.try_get("payload")?;
        if let Some(object) = payload.as_object_mut() {
            object.insert("publishedRevision".into(), json!(before_revision));
            object.insert(
                "status".into(),
                json!(if before_revision.is_some() {
                    "published"
                } else {
                    "archived"
                }),
            );
            object.insert("indexable".into(), json!(before_revision.is_some()));
            object.insert("updatedAt".into(), json!(now));
        }
        if let Some(revision) = before_revision {
            let localization = sqlx::query(
                r#"SELECT locale,seo_metadata,indexable,is_placeholder
                   FROM product_localizations
                   WHERE product_id=$1 AND product_revision=$2 AND locale='en'"#,
            )
            .bind(product_id)
            .bind(revision)
            .fetch_optional(&mut *transaction)
            .await?;
            let Some(localization) = localization else {
                skipped += 1;
                continue;
            };
            let seo: Value = localization.try_get("seo_metadata")?;
            let canonical_path = seo
                .get("canonicalPath")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ApiError::service_unavailable(
                        "A rollback target has no canonical product path.",
                    )
                })?;
            let indexable = localization.try_get::<bool, _>("indexable")?
                && !localization.try_get::<bool, _>("is_placeholder")?;
            sqlx::query("DELETE FROM public_routes WHERE entity_type='product' AND entity_id=$1")
                .bind(product_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                r#"UPDATE products SET status='published',published_revision=$2,
                          indexable=$3,payload=$4,updated_at=$5 WHERE id=$1"#,
            )
            .bind(product_id)
            .bind(revision)
            .bind(indexable)
            .bind(&payload)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"INSERT INTO public_routes
                   (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
                   VALUES ($1,'product',$2,'en',$3,$4,$5)"#,
            )
            .bind(Uuid::new_v4())
            .bind(product_id)
            .bind(canonical_path)
            .bind(indexable)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
                   VALUES ($1,'public.product.published','product',$2,$3)"#,
            )
            .bind(Uuid::new_v4())
            .bind(product_id)
            .bind(json!({
                "entityId": product_id,
                "factRevision": revision,
                "rollbackOf": sync_run_id
            }))
            .execute(&mut *transaction)
            .await?;
        } else {
            sqlx::query("DELETE FROM public_routes WHERE entity_type='product' AND entity_id=$1")
                .bind(product_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                r#"UPDATE products SET status='archived',published_revision=NULL,
                          indexable=false,payload=$2,updated_at=$3 WHERE id=$1"#,
            )
            .bind(product_id)
            .bind(&payload)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
                   VALUES ($1,'public.product.unpublished','product',$2,$3)"#,
            )
            .bind(Uuid::new_v4())
            .bind(product_id)
            .bind(json!({"entityId": product_id, "rollbackOf": sync_run_id}))
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            r#"UPDATE feishu_sync_changes SET rolled_back_at=$3,rolled_back_by=$4
               WHERE sync_run_id=$1 AND product_id=$2 AND rolled_back_at IS NULL"#,
        )
        .bind(sync_run_id)
        .bind(product_id)
        .bind(now)
        .bind(actor)
        .execute(&mut *transaction)
        .await?;
        restored += 1;
    }
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,
            request_id,occurred_at)
           VALUES ($1,$2,'feishu.sync.rollback','feishuSync',$3,NULL,$4,$5,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(sync_run_id)
    .bind(json!({"restored": restored, "skipped": skipped}))
    .bind("Restore published revisions from a completed Feishu synchronization")
    .bind(Uuid::new_v4())
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(FeishuRollbackReport {
        sync_run_id,
        restored,
        skipped,
        completed_at: now,
    })
}
