use super::*;

/// Promote every normalized row from one durable import run into immutable
/// draft product revisions. The transaction is all-or-nothing. A worker crash
/// after this commit is harmless: the completed run is returned on retry and
/// no second product revision is created.
pub async fn promote_staged_product_import(
    pool: &PgPool,
    import_run_id: Uuid,
    approved: Option<&ApprovedProductMaster>,
) -> Result<ProductImportResult, ApiError> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("verifiedCsv:promote:{import_run_id}"))
        .execute(&mut *transaction)
        .await?;
    let run = sqlx::query(
        r#"SELECT environment,source_checksum,mapping_version,status,created_by,
                  records_received,records_valid
           FROM product_import_runs
           WHERE id=$1 AND data_origin='verifiedCsv'
           FOR UPDATE"#,
    )
    .bind(import_run_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| ApiError::not_found("Product import run was not found."))?;
    let status: String = run.try_get("status")?;
    let environment: String = run.try_get("environment")?;
    let import_checksum: String = run.try_get("source_checksum")?;
    let mapping_version: String = run.try_get("mapping_version")?;
    let records_received: i64 = run.try_get("records_received")?;
    let records_valid: i64 = run.try_get("records_valid")?;
    verify_product_master_authority(
        &environment,
        approved,
        &import_checksum,
        &mapping_version,
        records_valid,
        records_received.saturating_sub(records_valid),
    )?;
    if status == "completed" {
        transaction.rollback().await?;
        return load_product_import_result(pool, import_run_id)
            .await?
            .ok_or_else(|| ApiError::service_unavailable("Stored product import is incomplete."));
    }
    if status == "failed" || status == "cancelled" {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "A failed or cancelled Product Master import cannot be promoted.",
        ));
    }
    let actor_id: Option<Uuid> = run.try_get("created_by")?;
    let now = Utc::now();
    sqlx::query("UPDATE product_import_runs SET status='validating' WHERE id=$1")
        .bind(import_run_id)
        .execute(&mut *transaction)
        .await?;
    let rows = sqlx::query(
        r#"SELECT normalized.source_record_id,private.source_row_number,
                  normalized.normalized_payload
           FROM product_import_normalized_records AS normalized
           JOIN product_import_private_staging AS private
             ON private.import_run_id=normalized.import_run_id
            AND private.source_record_id=normalized.source_record_id
           WHERE normalized.import_run_id=$1 AND normalized.validation_status='valid'
           ORDER BY private.source_row_number,normalized.source_record_id"#,
    )
    .bind(import_run_id)
    .fetch_all(&mut *transaction)
    .await?;
    if rows.is_empty() {
        return Err(ApiError::conflict(
            "Product import has no validated rows to promote.",
        ));
    }
    for stored in rows {
        let row = ImportedProductRow {
            row_number: stored.try_get("source_row_number")?,
            stable_id: stored.try_get("source_record_id")?,
            normalized_payload: stored.try_get("normalized_payload")?,
            confidential_payload: None,
        };
        promote_verified_csv_product(
            &mut transaction,
            import_run_id,
            &import_checksum,
            &row,
            actor_id,
            now,
        )
        .await?;
        sqlx::query(
            r#"UPDATE product_import_private_staging
               SET status='promoted',processed_at=$3
               WHERE import_run_id=$1 AND source_record_id=$2"#,
        )
        .bind(import_run_id)
        .bind(&row.stable_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query("UPDATE product_import_runs SET status='completed',completed_at=$2 WHERE id=$1")
        .bind(import_run_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    load_product_import_result(pool, import_run_id)
        .await?
        .ok_or_else(|| ApiError::service_unavailable("Stored product import is incomplete."))
}

/// Re-open a terminally failed import only while its authenticated private
/// staging rows still exist. This is called in the same transaction that
/// resets the durable job and operation, preventing a queued job from pointing
/// at an import run the worker must reject.
pub async fn reset_staged_product_import_for_retry(
    transaction: &mut Transaction<'_, Postgres>,
    import_run_id: Uuid,
) -> Result<(), ApiError> {
    let reset = sqlx::query(
        r#"UPDATE product_import_runs AS import
           SET status='readyToPublish',completed_at=NULL
           WHERE import.id=$1 AND import.status='failed'
             AND EXISTS (
                 SELECT 1 FROM product_import_normalized_records AS normalized
                 JOIN product_import_private_staging AS private
                   ON private.import_run_id=normalized.import_run_id
                  AND private.source_record_id=normalized.source_record_id
                 WHERE normalized.import_run_id=import.id
                   AND normalized.validation_status='valid'
                   AND private.status='validated'
                   AND private.expires_at > now()
             )"#,
    )
    .bind(import_run_id)
    .execute(&mut **transaction)
    .await?;
    if reset.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "Product import staging is missing, expired, or not retryable.",
        ));
    }
    Ok(())
}

pub(super) async fn promote_verified_csv_product(
    transaction: &mut Transaction<'_, Postgres>,
    import_run_id: Uuid,
    import_checksum: &str,
    row: &ImportedProductRow,
    actor_id: Option<Uuid>,
    now: chrono::DateTime<Utc>,
) -> Result<(), ApiError> {
    let existing = sqlx::query(
        "SELECT id,current_revision,published_revision,data_origin FROM products WHERE stable_id=$1 FOR UPDATE",
    )
    .bind(&row.stable_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let (product_id, revision, published_revision): (Uuid, i64, Option<i64>) = match existing {
        Some(existing) => {
            let origin: String = existing.try_get("data_origin")?;
            if origin != "verifiedCsv" {
                return Err(ApiError::conflict(format!(
                    "Product `{}` is owned by {origin} and cannot be overwritten by verified CSV.",
                    row.stable_id
                )));
            }
            (
                existing.try_get("id")?,
                existing.try_get::<i64, _>("current_revision")? + 1,
                existing.try_get("published_revision")?,
            )
        }
        None => (Uuid::new_v4(), 1_i64, None),
    };
    let mut payload = row
        .normalized_payload
        .as_object()
        .cloned()
        .ok_or_else(|| ApiError::internal("Normalized product payload is not an object."))?;
    payload.insert("id".into(), json!(product_id));
    payload.insert("sourceSnapshotId".into(), json!(Uuid::nil()));
    payload.insert(
        "sourceRevision".into(),
        json!(format!("csv:{import_checksum}")),
    );
    payload.insert("currentRevision".into(), json!(revision));
    payload.insert("publishedRevision".into(), json!(published_revision));
    payload.insert("status".into(), json!("draft"));
    payload.insert("indexable".into(), json!(false));
    payload.insert("updatedAt".into(), json!(now));
    payload.insert("dataOrigin".into(), json!("verifiedCsv"));
    let payload = Value::Object(payload);
    let model = payload.get("model").and_then(Value::as_str);
    let slug = required_payload_text(&payload, "slug")?;
    let locale = required_payload_text(&payload, "locale")?;
    let family = required_payload_text(&payload, "family")?;
    let title = required_payload_text(&payload, "title")?;
    let summary = payload.get("summary").and_then(Value::as_str);

    if revision == 1 {
        sqlx::query(
            r#"INSERT INTO products
               (id,stable_id,model,slug,locale,family,source_snapshot_id,source_revision,
                status,current_revision,published_revision,indexable,payload,updated_at,
                data_origin,product_import_run_id)
               VALUES ($1,$2,$3,$4,$5,$6,NULL,$7,'draft',1,NULL,false,$8,$9,
                       'verifiedCsv',$10)"#,
        )
        .bind(product_id)
        .bind(&row.stable_id)
        .bind(model)
        .bind(slug)
        .bind(locale)
        .bind(family)
        .bind(format!("csv:{import_checksum}"))
        .bind(&payload)
        .bind(now)
        .bind(import_run_id)
        .execute(&mut **transaction)
        .await?;
    } else {
        sqlx::query(
            r#"UPDATE products SET model=$2,slug=$3,locale=$4,family=$5,
                      source_snapshot_id=NULL,source_revision=$6,status='draft',
                      current_revision=$7,indexable=false,payload=$8,updated_at=$9,
                      data_origin='verifiedCsv',product_import_run_id=$10
               WHERE id=$1"#,
        )
        .bind(product_id)
        .bind(model)
        .bind(slug)
        .bind(locale)
        .bind(family)
        .bind(format!("csv:{import_checksum}"))
        .bind(revision)
        .bind(&payload)
        .bind(now)
        .bind(import_run_id)
        .execute(&mut **transaction)
        .await?;
    }
    sqlx::query(
        r#"INSERT INTO product_revisions
           (product_id,revision,source_snapshot_id,payload,created_at,data_origin,product_import_run_id)
           VALUES ($1,$2,NULL,$3,$4,'verifiedCsv',$5)"#,
    )
    .bind(product_id)
    .bind(revision)
    .bind(&payload)
    .bind(now)
    .bind(import_run_id)
    .execute(&mut **transaction)
    .await?;
    // Product Master updates advance only the immutable facts revision. The
    // portal-owned website presentation is initialized once and then remains
    // independent across every later CSV/Feishu import.
    if revision == 1 {
        let presentation_actor = actor_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "productImportWorker".into());
        let presentation_seo = json!({
            "title": null,
            "description": null,
            "canonicalPath": null,
            "indexable": false
        });
        sqlx::query(
            r#"INSERT INTO product_presentation_working
               (product_id,locale,current_revision,published_revision,slug,title,summary,
                content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                updated_by,updated_at)
               VALUES ($1,$2,1,NULL,$3,$4,$5,'{}'::jsonb,$6,'draft',false,false,
                       'verifiedCsv',$7,$8)"#,
        )
        .bind(product_id)
        .bind(locale)
        .bind(slug)
        .bind(title)
        .bind(summary)
        .bind(&presentation_seo)
        .bind(&presentation_actor)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO product_presentation_revisions
               (product_id,locale,revision,source_product_revision,slug,title,summary,
                content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
                created_by,created_at)
               VALUES ($1,$2,1,1,$3,$4,$5,'{}'::jsonb,$6,'draft',false,false,
                       'verifiedCsv',$7,$8)"#,
        )
        .bind(product_id)
        .bind(locale)
        .bind(slug)
        .bind(title)
        .bind(summary)
        .bind(&presentation_seo)
        .bind(&presentation_actor)
        .bind(now)
        .execute(&mut **transaction)
        .await?;
    }
    project_product_facts(
        transaction,
        product_id,
        revision,
        &payload,
        &format!("verified-csv:{import_checksum}:{}", row.stable_id),
    )
    .await?;
    Ok(())
}
