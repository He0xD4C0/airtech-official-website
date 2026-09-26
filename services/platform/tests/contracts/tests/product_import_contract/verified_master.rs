use super::*;

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to empty disposable PostgreSQL and AIRTEK_PRODUCT_MASTER_TEST_CSV"]
async fn verified_master_is_staged_queued_promoted_and_replayed_without_plaintext_jobs() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to empty disposable PostgreSQL");
    let csv_path = std::env::var("AIRTEK_PRODUCT_MASTER_TEST_CSV")
        .expect("AIRTEK_PRODUCT_MASTER_TEST_CSV must identify the confirmed Product Master");
    let csv = std::fs::read_to_string(csv_path).expect("Product Master CSV is readable");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(crate::support::disposable_database_url(&database_url))
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM products")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "this contract requires an empty disposable database"
    );

    let mut config = Config::for_test();
    config.database_url = Some(database_url.clone());
    let parsed = parse_product_master(
        &csv,
        "airtek-basic-v1",
        config.product_staging_encryption_key.as_ref(),
    )
    .expect("confirmed Product Master parses");
    let staged = stage_and_queue_product_import(&pool, parsed, "test", None, None, "contractTest")
        .await
        .expect("encrypted staging transaction");
    assert!(staged.queued);
    assert_eq!(staged.result.total_rows, 375);
    assert_eq!(staged.result.valid_rows, 370);
    assert_eq!(staged.result.malformed_rows, 5);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT environment FROM product_import_runs WHERE id=$1",)
            .bind(staged.result.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "test"
    );

    let job = sqlx::query("SELECT status,payload FROM jobs WHERE id=$1")
        .bind(staged.operation_id)
        .fetch_one(&pool)
        .await
        .expect("durable job");
    assert_eq!(job.try_get::<String, _>("status").unwrap(), "queued");
    assert_eq!(
        job.try_get::<Value, _>("payload").unwrap(),
        serde_json::json!({"importRunId": staged.operation_id})
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_import_private_staging WHERE import_run_id=$1",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        370
    );
    let private = sqlx::query(
        r#"SELECT private.source_row_number,private.nonce,private.ciphertext,
                  private.authentication_tag,run.mapping_version,run.source_checksum
           FROM product_import_private_staging AS private
           JOIN product_import_runs AS run ON run.id=private.import_run_id
           WHERE private.import_run_id=$1 AND private.source_record_id='B23E280H128-102-B0'"#,
    )
    .bind(staged.result.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let nonce: Vec<u8> = private.try_get("nonce").unwrap();
    let ciphertext: Vec<u8> = private.try_get("ciphertext").unwrap();
    let authentication_tag: Vec<u8> = private.try_get("authentication_tag").unwrap();
    let mapping_version: String = private.try_get("mapping_version").unwrap();
    let checksum: String = private.try_get("source_checksum").unwrap();
    let pricing = decrypt_private_pricing(
        config.product_staging_encryption_key.as_ref().unwrap(),
        PrivatePricingEnvelope {
            mapping_version: &mapping_version,
            checksum: &checksum,
            source_row_number: private.try_get("source_row_number").unwrap(),
            stable_id: "B23E280H128-102-B0",
            nonce: &nonce,
            ciphertext: &ciphertext,
            authentication_tag: &authentication_tag,
        },
    )
    .expect("private pricing decrypts only through row-bound AAD");
    assert_eq!(pricing.len(), 5);
    assert!(pricing.keys().all(|name| !name.contains("noise")));
    let duplicate_nonce_error = sqlx::query(
        r#"INSERT INTO product_import_private_staging
               (id,import_run_id,source_record_id,source_row_number,ciphertext,
                encryption_algorithm,encryption_key_id,nonce,authentication_tag,
                checksum,status,created_at,expires_at,processed_at)
           SELECT $2,import_run_id,source_record_id || '-nonce-collision',
                  source_row_number + 100000,ciphertext,encryption_algorithm,
                  encryption_key_id,nonce,authentication_tag,checksum,status,
                  created_at,expires_at,processed_at
           FROM product_import_private_staging
           WHERE import_run_id=$1 AND source_record_id='B23E280H128-102-B0'"#,
    )
    .bind(staged.result.id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .expect_err("an active AES-GCM nonce must be unique within its encryption key");
    assert_eq!(
        duplicate_nonce_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23505")
    );
    let expiry_index = sqlx::query_scalar::<_, String>(
        r#"SELECT indexdef FROM pg_indexes
           WHERE schemaname=current_schema()
             AND indexname='product_import_private_staging_expiry_idx'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("private staging expiry index");
    assert!(expiry_index.contains("promoted"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_import_normalized_records WHERE import_run_id=$1",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        370
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM products")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "request-time staging must not synchronously create product revisions"
    );

    // A terminal worker failure can be retried only while authenticated
    // private staging remains available. The reset occurs atomically with the
    // job/operation state reset for the import workflow.
    let mut retry = pool.begin().await.unwrap();
    sqlx::query("UPDATE jobs SET status='failed' WHERE id=$1")
        .bind(staged.operation_id)
        .execute(&mut *retry)
        .await
        .unwrap();
    sqlx::query("UPDATE operation_runs SET status='failed' WHERE id=$1")
        .bind(staged.operation_id)
        .execute(&mut *retry)
        .await
        .unwrap();
    sqlx::query("UPDATE product_import_runs SET status='failed' WHERE id=$1")
        .bind(staged.operation_id)
        .execute(&mut *retry)
        .await
        .unwrap();
    reset_staged_product_import_for_retry(&mut retry, staged.operation_id)
        .await
        .expect("failed import staging is retryable before expiry");
    sqlx::query("UPDATE jobs SET status='queued',attempts=0 WHERE id=$1")
        .bind(staged.operation_id)
        .execute(&mut *retry)
        .await
        .unwrap();
    sqlx::query("UPDATE operation_runs SET status='queued' WHERE id=$1")
        .bind(staged.operation_id)
        .execute(&mut *retry)
        .await
        .unwrap();
    retry.commit().await.unwrap();

    let state = AppState::new(config).expect("worker state");
    run_one_pending_job(&state)
        .await
        .expect("worker promotion job");
    let lifecycle = sqlx::query(
        r#"SELECT job.status AS job_status,operation.status AS operation_status,
                  import.status AS import_status
           FROM jobs AS job
           JOIN operation_runs AS operation ON operation.id=job.id
           JOIN product_import_runs AS import ON import.id=job.id
           WHERE job.id=$1"#,
    )
    .bind(staged.operation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        lifecycle.try_get::<String, _>("job_status").unwrap(),
        "completed"
    );
    assert_eq!(
        lifecycle.try_get::<String, _>("operation_status").unwrap(),
        "completed"
    );
    assert_eq!(
        lifecycle.try_get::<String, _>("import_status").unwrap(),
        "completed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM products WHERE product_import_run_id=$1 AND status='draft' AND published_revision IS NULL",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        370
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_import_errors WHERE import_run_id=$1 AND severity='error'",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        5
    );
    let localization_seo = sqlx::query_scalar::<_, Value>(
        r#"SELECT presentation.seo_metadata
           FROM product_presentation_working AS presentation
           JOIN products AS product ON product.id=presentation.product_id
           WHERE product.product_import_run_id=$1
           ORDER BY presentation.product_id LIMIT 1"#,
    )
    .bind(staged.result.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let localization_seo: SeoMetadata = serde_json::from_value(localization_seo)
        .expect("an imported product presentation has a complete typed SEO object");
    assert!(!localization_seo.indexable);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM product_localizations localization
               JOIN products product ON product.id=localization.product_id
               WHERE product.product_import_run_id=$1"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "draft imports must not leak into the public localization projection"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM audit_log WHERE entity_id=$1 AND action='product.sourceResolution'",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );

    let before_revisions = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_revisions")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE product_import_runs SET environment='legacy' WHERE id=$1")
        .bind(staged.result.id)
        .execute(&pool)
        .await
        .expect("simulate a pre-0008 import run");
    let replay_parsed = parse_product_master(
        &csv,
        "airtek-basic-v1",
        state.config.product_staging_encryption_key.as_ref(),
    )
    .unwrap();
    let replay =
        stage_and_queue_product_import(&pool, replay_parsed, "test", None, None, "contractTest")
            .await
            .unwrap();
    assert!(replay.result.reused);
    assert!(!replay.queued);
    assert_eq!(replay.operation_id, staged.operation_id);
    assert_eq!(replay.result.status, "completed");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT environment FROM product_import_runs WHERE id=$1",)
            .bind(staged.result.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "test",
        "the first matching runtime environment claims a legacy run without duplication"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM audit_log
               WHERE entity_id=$1 AND action='productMaster.import.environmentClaim'"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_revisions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        before_revisions
    );

    let other_environment_parsed = parse_product_master(
        &csv,
        "airtek-basic-v1",
        state.config.product_staging_encryption_key.as_ref(),
    )
    .unwrap();
    let other_environment = stage_and_queue_product_import(
        &pool,
        other_environment_parsed,
        "development",
        None,
        None,
        "contractTest",
    )
    .await
    .expect("the same authoritative file has an independent development idempotency scope");
    assert!(!other_environment.result.reused);
    assert_ne!(other_environment.operation_id, staged.operation_id);
}
