/// Persist the encrypted source rows and the allow-listed normalized rows,
/// then enqueue a worker job which refers only to the import run UUID.
///
/// The caller must parse with [`parse_product_master`] before entering this
/// function. Plaintext CSV and plaintext source rows never enter
/// `operation_runs`, `jobs`, `audit_log`, or tracing fields.
pub async fn stage_and_queue_product_import(
    pool: &PgPool,
    mut parsed: ParsedProductImport,
    environment: &str,
    approved: Option<&ApprovedProductMaster>,
    actor_id: Option<Uuid>,
    audit_actor: &str,
) -> Result<StagedProductImport, ApiError> {
    let authority_decision = verify_product_master_authority(
        environment,
        approved,
        &parsed.result.checksum,
        &parsed.result.mapping_version,
        parsed.result.valid_rows,
        parsed.result.malformed_rows,
    )?;
    parsed.result.status = if parsed.result.valid_rows > 0 {
        "readyToPublish".into()
    } else {
        "failed".into()
    };
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "verifiedCsv:{environment}:{}:{}",
            parsed.result.checksum, parsed.result.mapping_version,
        ))
        .execute(&mut *transaction)
        .await?;

    let mut existing = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM product_import_runs
           WHERE environment=$1 AND data_origin='verifiedCsv'
             AND source_checksum=$2 AND mapping_version=$3"#,
    )
    .bind(environment)
    .bind(&parsed.result.checksum)
    .bind(&parsed.result.mapping_version)
    .fetch_optional(&mut *transaction)
    .await?;
    let mut claimed_legacy = false;
    if existing.is_none() {
        let legacy = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM product_import_runs
               WHERE environment='legacy' AND data_origin='verifiedCsv'
                 AND source_checksum=$1 AND mapping_version=$2
               ORDER BY created_at,id
               FOR UPDATE LIMIT 1"#,
        )
        .bind(&parsed.result.checksum)
        .bind(&parsed.result.mapping_version)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(legacy_id) = legacy {
            existing = sqlx::query_scalar::<_, Uuid>(
                r#"UPDATE product_import_runs SET environment=$2
                   WHERE id=$1 AND environment='legacy'
                   RETURNING id"#,
            )
            .bind(legacy_id)
            .bind(environment)
            .fetch_optional(&mut *transaction)
            .await?;
            claimed_legacy = existing.is_some();
            if claimed_legacy {
                sqlx::query(
                    r#"INSERT INTO audit_log
                       (id,actor,action,entity_type,entity_id,before_value,
                        after_value,reason,request_id,occurred_at)
                       VALUES ($1,$2,'productMaster.import.environmentClaim',
                               'productImport',$3,$4,$5,$6,$7,$8)"#,
                )
                .bind(Uuid::new_v4())
                .bind(audit_actor)
                .bind(legacy_id)
                .bind(json!({"environment": "legacy"}))
                .bind(json!({"environment": environment, "actorId": actor_id}))
                .bind("Attribute a pre-0008 Product import to its first verified runtime environment.")
                .bind(Uuid::new_v4())
                .bind(Utc::now())
                .execute(&mut *transaction)
                .await?;
            }
        }
    }

    if let Some(existing) = existing {
        if claimed_legacy {
            transaction.commit().await?;
        } else {
            transaction.rollback().await?;
        }
        let mut result = load_product_import_result(pool, existing)
            .await?
            .ok_or_else(|| ApiError::service_unavailable("Stored product import is incomplete."))?;
        verify_product_master_authority(
            environment,
            approved,
            &result.checksum,
            &result.mapping_version,
            result.valid_rows,
            result.malformed_rows,
        )?;
        result.reused = true;
        let queued = matches!(
            result.status.as_str(),
            "queued" | "receiving" | "validating" | "readyToPublish"
        );
        return Ok(StagedProductImport {
            operation_id: existing,
            result,
            queued,
        });
    }

    let created_at = parsed.result.created_at;
    let run_id = parsed.result.id;
    let queued = parsed.result.valid_rows > 0;
    let error_count = parsed
        .result
        .errors
        .iter()
        .filter(|error| error.severity == "error")
        .count() as i64;
    sqlx::query(
        r#"INSERT INTO product_import_runs
           (id,environment,data_origin,dry_run,status,mapping_version,source_checksum,
            records_received,records_valid,error_count,created_by,started_at,completed_at,created_at)
           VALUES ($1,$2,'verifiedCsv',false,$3,$4,$5,$6,$7,$8,$9,$10,
                   CASE WHEN $11 THEN NULL ELSE $10 END,$10)"#,
    )
    .bind(run_id)
    .bind(environment)
    .bind(&parsed.result.status)
    .bind(&parsed.result.mapping_version)
    .bind(&parsed.result.checksum)
    .bind(parsed.result.total_rows)
    .bind(parsed.result.valid_rows)
    .bind(error_count)
    .bind(actor_id)
    .bind(created_at)
    .bind(queued)
    .execute(&mut *transaction)
    .await?;

    for row in &parsed.rows {
        persist_staged_row(&mut transaction, &parsed, row).await?;
    }
    persist_import_findings(&mut transaction, &parsed).await?;
    persist_source_resolution_audit(&mut transaction, &parsed, audit_actor, actor_id).await?;

    let operation_status = if queued { "queued" } else { "failed" };
    let operation_result = (!queued).then(|| json!({"import": parsed.result}));
    sqlx::query(
        r#"INSERT INTO operation_runs
           (id,kind,status,reason,result,created_at,updated_at)
           VALUES ($1,'productImport',$2,$3,$4,$5,$5)"#,
    )
    .bind(run_id)
    .bind(operation_status)
    .bind("Import verified Product Master CSV")
    .bind(operation_result)
    .bind(created_at)
    .execute(&mut *transaction)
    .await?;
    if queued {
        // This is the complete durable job payload. In particular, it must
        // never grow to include CSV text, normalized product facts, filenames,
        // prices, or encrypted source data.
        sqlx::query(
            r#"INSERT INTO jobs
               (id,job_type,status,payload,available_at,created_at,updated_at)
               VALUES ($1,'productImport','queued',$2,$3,$3,$3)"#,
        )
        .bind(run_id)
        .bind(product_import_job_payload(run_id))
        .bind(created_at)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,'productMaster.import.accept','productImport',$3,NULL,$4,$5,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(audit_actor)
    .bind(run_id)
    .bind(json!({
        "operationId": run_id,
        "environment": environment,
        "sourceChecksum": parsed.result.checksum,
        "mappingVersion": parsed.result.mapping_version,
        "totalRows": parsed.result.total_rows,
        "validRows": parsed.result.valid_rows,
        "malformedRows": parsed.result.malformed_rows,
        "authorityAdmission": authority_decision.audit_label(),
        "queued": queued
    }))
    .bind("Accept encrypted verified Product Master staging and durable promotion job.")
    .bind(Uuid::new_v4())
    .bind(created_at)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(StagedProductImport {
        operation_id: run_id,
        result: parsed.result,
        queued,
    })
}

fn product_import_job_payload(import_run_id: Uuid) -> Value {
    json!({"importRunId": import_run_id})
}

async fn persist_staged_row(
    transaction: &mut Transaction<'_, Postgres>,
    parsed: &ParsedProductImport,
    row: &ImportedProductRow,
) -> Result<(), ApiError> {
    let encrypted = row.confidential_payload.as_deref().ok_or_else(|| {
        ApiError::internal("A valid Product Master row is missing encrypted staging data.")
    })?;
    if encrypted.len() < 28 {
        return Err(ApiError::internal(
            "Encrypted Product Master row is invalid.",
        ));
    }
    let nonce = &encrypted[..12];
    let tag = &encrypted[encrypted.len() - 16..];
    let ciphertext = &encrypted[12..encrypted.len() - 16];
    sqlx::query(
        r#"INSERT INTO product_import_private_staging
           (id,import_run_id,source_record_id,source_row_number,ciphertext,encryption_algorithm,
            encryption_key_id,nonce,authentication_tag,checksum,status,created_at,expires_at)
           VALUES ($1,$2,$3,$4,$5,'AES-256-GCM','environment-v1',$6,$7,$8,
                   'validated',$9,$10)"#,
    )
    .bind(Uuid::new_v4())
    .bind(parsed.result.id)
    .bind(&row.stable_id)
    .bind(row.row_number)
    .bind(ciphertext)
    .bind(nonce)
    .bind(tag)
    .bind(format!("{:x}", Sha256::digest(encrypted)))
    .bind(parsed.result.created_at)
    .bind(parsed.result.created_at + Duration::days(30))
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO product_import_normalized_records
           (id,import_run_id,source_record_id,normalized_payload,validation_status,created_at)
           VALUES ($1,$2,$3,$4,'valid',$5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(parsed.result.id)
    .bind(&row.stable_id)
    .bind(&row.normalized_payload)
    .bind(parsed.result.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_import_findings(
    transaction: &mut Transaction<'_, Postgres>,
    parsed: &ParsedProductImport,
) -> Result<(), ApiError> {
    for error in &parsed.result.errors {
        sqlx::query(
            r#"INSERT INTO product_import_errors
               (id,import_run_id,source_record_id,severity,error_code,field_path,message,details,created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(Uuid::new_v4())
        .bind(parsed.result.id)
        .bind(error.stable_id.as_deref())
        .bind(&error.severity)
        .bind(&error.code)
        .bind(error.field_name.as_deref())
        .bind(&error.detail)
        .bind(json!({"rowNumber": error.row_number}))
        .bind(parsed.result.created_at)
        .execute(&mut **transaction)
        .await?;
    }
    for missing in &parsed.result.missing_assets {
        sqlx::query(
            r#"INSERT INTO product_import_missing_assets
               (id,import_run_id,source_record_id,asset_type,source_reference,
                resolution_status,created_at)
               VALUES ($1,$2,$3,$4,$5,'missing',$6)"#,
        )
        .bind(Uuid::new_v4())
        .bind(parsed.result.id)
        .bind(&missing.stable_id)
        .bind(&missing.asset_type)
        .bind(&missing.source_reference)
        .bind(parsed.result.created_at)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

async fn persist_source_resolution_audit(
    transaction: &mut Transaction<'_, Postgres>,
    parsed: &ParsedProductImport,
    audit_actor: &str,
    actor_id: Option<Uuid>,
) -> Result<(), ApiError> {
    if !parsed
        .rows
        .iter()
        .any(|row| row.stable_id == "B23E280H128-102-B0")
    {
        return Ok(());
    }
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,
            reason,request_id,occurred_at)
           VALUES ($1,$2,'product.sourceResolution','productImport',$3,NULL,$4,$5,$6,$7)"#,
    )
    .bind(Uuid::new_v4())
    .bind(audit_actor)
    .bind(parsed.result.id)
    .bind(json!({
        "stableId": "B23E280H128-102-B0",
        "acceptedAuthority": "verifiedProductMasterCsv",
        "verifiedCsvChecksum": parsed.result.checksum,
        "mappingVersion": parsed.result.mapping_version,
        "acceptedBy": actor_id,
        "excludedSourceClass": "interactionDemo"
    }))
    .bind("The confirmed Product Master CSV is the accepted source; interaction demos supplied no product facts.")
    .bind(Uuid::new_v4())
    .bind(parsed.result.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
