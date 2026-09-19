use std::collections::BTreeSet;

use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{error::ApiError, models::FeishuSource, state::AppState};

pub(super) async fn mark_removed_sources_pending(
    transaction: &mut Transaction<'_, Postgres>,
    connector_id: Uuid,
    before: &[FeishuSource],
    after: &[FeishuSource],
) -> Result<(), ApiError> {
    let retained = after
        .iter()
        .filter(|source| source.enabled)
        .map(source_identity)
        .collect::<BTreeSet<_>>();
    for source in before.iter().filter(|source| source.enabled) {
        if !retained.contains(&source_identity(source)) {
            mark_table_pending(
                transaction,
                connector_id,
                &source.wiki_token,
                &source.table_id,
                None,
            )
            .await?;
        }
    }
    Ok(())
}

pub(super) async fn activate_product_ownership(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    connector_id: Uuid,
    wiki_token: &str,
    table_id: &str,
    record_id: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO feishu_product_ownership
           (product_id,connector_id,wiki_token,table_id,record_id,generation,state,
            pending_delete_at,deletion_run_id,updated_at)
           VALUES ($1,$2,$3,$4,$5,1,'active',NULL,NULL,now())
           ON CONFLICT(product_id) DO UPDATE SET
             connector_id=EXCLUDED.connector_id,wiki_token=EXCLUDED.wiki_token,
             table_id=EXCLUDED.table_id,
             record_id=EXCLUDED.record_id,generation=feishu_product_ownership.generation+1,
             state='active',pending_delete_at=NULL,deletion_run_id=NULL,updated_at=now()"#,
    )
    .bind(product_id)
    .bind(connector_id)
    .bind(wiki_token)
    .bind(table_id)
    .bind(record_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn reconcile_missing_records(
    state: &AppState,
    run_id: Uuid,
    connector_id: Uuid,
    wiki_token: &str,
    table_id: &str,
) -> Result<u64, ApiError> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "feishu:reconcile:{connector_id}:{wiki_token}:{table_id}"
        ))
        .execute(&mut *transaction)
        .await?;
    let deleted = mark_table_pending(
        &mut transaction,
        connector_id,
        wiki_token,
        table_id,
        Some(run_id),
    )
    .await?;
    sqlx::query(
        r#"UPDATE feishu_run_table_results SET records_deleted=$4
           WHERE sync_run_id=$1 AND wiki_token=$2 AND table_id=$3"#,
    )
    .bind(run_id)
    .bind(wiki_token)
    .bind(table_id)
    .bind(i64::try_from(deleted).unwrap_or(i64::MAX))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(deleted)
}

async fn mark_table_pending(
    transaction: &mut Transaction<'_, Postgres>,
    connector_id: Uuid,
    wiki_token: &str,
    table_id: &str,
    missing_from_run: Option<Uuid>,
) -> Result<u64, ApiError> {
    let rows = sqlx::query(
        r#"SELECT ownership.product_id,product.current_revision
           FROM feishu_product_ownership ownership
           JOIN products product ON product.id=ownership.product_id
           WHERE ownership.connector_id=$1 AND ownership.wiki_token=$2
             AND ownership.table_id=$3
             AND ownership.state='active'
             AND ($4::uuid IS NULL OR NOT EXISTS (
               SELECT 1 FROM staging_records staging
               JOIN source_snapshots snapshot ON snapshot.id=staging.source_snapshot_id
               WHERE staging.sync_run_id=$4
                 AND snapshot.source_payload->>'wikiToken'=ownership.wiki_token
                 AND snapshot.source_payload->>'tableId'=ownership.table_id
                 AND staging.normalized_payload->>'sourceRecordId'=ownership.record_id
             ))
           FOR UPDATE OF ownership,product"#,
    )
    .bind(connector_id)
    .bind(wiki_token)
    .bind(table_id)
    .bind(missing_from_run)
    .fetch_all(&mut **transaction)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let product_ids = rows
        .iter()
        .map(|row| row.try_get::<Uuid, _>("product_id"))
        .collect::<Result<Vec<_>, _>>()?;
    sqlx::query(
        r#"UPDATE feishu_product_ownership
           SET state='pendingDelete',pending_delete_at=now(),deletion_run_id=$4,
               generation=generation+1,updated_at=now()
           WHERE connector_id=$1 AND wiki_token=$2 AND table_id=$3
             AND product_id=ANY($5)"#,
    )
    .bind(connector_id)
    .bind(wiki_token)
    .bind(table_id)
    .bind(missing_from_run)
    .bind(&product_ids)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"UPDATE products SET status='archived',published_revision=NULL,indexable=false,
             payload=payload || '{"status":"archived","publishedRevision":null,"indexable":false}'::jsonb,
             updated_at=now()
           WHERE id=ANY($1)"#,
    )
    .bind(&product_ids)
    .execute(&mut **transaction)
    .await?;
    sqlx::query("DELETE FROM public_routes WHERE entity_type='product' AND entity_id=ANY($1)")
        .bind(&product_ids)
        .execute(&mut **transaction)
        .await?;
    for row in &rows {
        let product_id: Uuid = row.try_get("product_id")?;
        let revision: i64 = row.try_get("current_revision")?;
        sqlx::query(
            r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
               VALUES ($1,'public.product.unpublished','product',$2,$3)"#,
        )
        .bind(Uuid::new_v4())
        .bind(product_id)
        .bind(json!({"entityId": product_id, "revision": revision, "reason": "sourceDeleted"}))
        .execute(&mut **transaction)
        .await?;
    }
    queue_purge(transaction, connector_id, wiki_token, table_id).await?;
    Ok(product_ids.len() as u64)
}

async fn queue_purge(
    transaction: &mut Transaction<'_, Postgres>,
    connector_id: Uuid,
    wiki_token: &str,
    table_id: &str,
) -> Result<(), ApiError> {
    let active: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(SELECT 1 FROM jobs
           WHERE job_type='feishuSourcePurge' AND status IN ('queued','running')
             AND payload->>'connectorId'=$1::text
             AND payload->>'wikiToken'=$2 AND payload->>'tableId'=$3)"#,
    )
    .bind(connector_id)
    .bind(wiki_token)
    .bind(table_id)
    .fetch_one(&mut **transaction)
    .await?;
    if active {
        return Ok(());
    }
    let id = Uuid::new_v4();
    let payload = json!({
        "connectorId": connector_id,
        "wikiToken": wiki_token,
        "tableId": table_id
    });
    sqlx::query(
        r#"INSERT INTO operation_runs(id,kind,status,reason,result,created_at,updated_at)
           VALUES ($1,'feishuSync','queued',$2,NULL,now(),now())"#,
    )
    .bind(id)
    .bind(format!(
        "Purge products removed from Feishu source {wiki_token}/{table_id}"
    ))
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO jobs
           (id,job_type,status,payload,connector_id,available_at,created_at,updated_at)
           VALUES ($1,'feishuSourcePurge','queued',$2,$3,now(),now(),now())"#,
    )
    .bind(id)
    .bind(payload)
    .bind(connector_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn execute_source_purge(
    state: &AppState,
    connector_id: Uuid,
    wiki_token: &str,
    table_id: &str,
) -> Result<Value, String> {
    execute_source_purge_inner(state, connector_id, wiki_token, table_id)
        .await
        .map_err(|error| error.to_string())
}

async fn execute_source_purge_inner(
    state: &AppState,
    connector_id: Uuid,
    wiki_token: &str,
    table_id: &str,
) -> Result<Value, ApiError> {
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "feishu:purge:{connector_id}:{wiki_token}:{table_id}"
        ))
        .execute(&mut *transaction)
        .await?;
    let rows = sqlx::query(
        r#"SELECT ownership.product_id,product.stable_id
           FROM feishu_product_ownership ownership
           JOIN products product ON product.id=ownership.product_id
           WHERE ownership.connector_id=$1 AND ownership.wiki_token=$2
             AND ownership.table_id=$3
             AND ownership.state='pendingDelete'
           FOR UPDATE OF ownership,product"#,
    )
    .bind(connector_id)
    .bind(wiki_token)
    .bind(table_id)
    .fetch_all(&mut *transaction)
    .await?;
    let product_ids = rows
        .iter()
        .map(|row| row.try_get::<Uuid, _>("product_id"))
        .collect::<Result<Vec<_>, _>>()?;
    let stable_ids = rows
        .iter()
        .map(|row| row.try_get::<String, _>("stable_id"))
        .collect::<Result<Vec<_>, _>>()?;
    let asset_ids = if product_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT DISTINCT media_asset_id FROM asset_references WHERE product_id=ANY($1)",
        )
        .bind(&product_ids)
        .fetch_all(&mut *transaction)
        .await?
    };
    if !product_ids.is_empty() {
        super::queue_purge_asset_cleanup(&mut transaction, &asset_ids, &product_ids).await?;
        sqlx::query(
            "DELETE FROM cms_current_publication_dependencies WHERE target_product_id=ANY($1)",
        )
        .bind(&product_ids)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM products WHERE id=ANY($1)")
            .bind(&product_ids)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM product_import_private_staging WHERE source_record_id=ANY($1)")
            .bind(&stable_ids)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM product_import_errors WHERE source_record_id=ANY($1)")
            .bind(&stable_ids)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM staging_records WHERE source_record_id=ANY($1)")
            .bind(&stable_ids)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            r#"DELETE FROM source_snapshots snapshot WHERE source_record_id=ANY($1)
               AND NOT EXISTS(SELECT 1 FROM product_revisions revision
                              WHERE revision.source_snapshot_id=snapshot.id)
               AND NOT EXISTS(SELECT 1 FROM products product
                              WHERE product.source_snapshot_id=snapshot.id)
               AND NOT EXISTS(SELECT 1 FROM staging_records staging
                              WHERE staging.source_snapshot_id=snapshot.id)"#,
        )
        .bind(&stable_ids)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(json!({"connectorId": connector_id, "wikiToken": wiki_token,
              "tableId": table_id, "deleted": product_ids.len()}))
}

pub fn parse_purge_payload(payload: &Value) -> Result<(Uuid, String, String), String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "Feishu purge payload must be an object.".to_owned())?;
    if object.len() != 3 {
        return Err(
            "Feishu purge payload must contain connectorId, wikiToken, and tableId.".into(),
        );
    }
    let connector_id = object
        .get("connectorId")
        .and_then(Value::as_str)
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| "Feishu purge connectorId is invalid.".to_owned())?;
    let wiki_token = object
        .get("wikiToken")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Feishu purge wikiToken is invalid.".to_owned())?;
    let table_id = object
        .get("tableId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Feishu purge tableId is invalid.".to_owned())?;
    Ok((connector_id, wiki_token, table_id))
}

fn source_identity(source: &FeishuSource) -> (String, String) {
    (
        source.wiki_token.trim().to_owned(),
        source.table_id.trim().to_owned(),
    )
}
