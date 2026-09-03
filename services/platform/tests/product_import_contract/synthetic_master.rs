#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn synthetic_product_master_scale_is_private_promoted_draft_and_idempotent() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;

    let prefix = Uuid::new_v4().simple().to_string();
    let master = synthetic_product_master(&prefix);
    let mut config = Config::for_test();
    config.database_url = Some(database_url);
    let parsed = parse_product_master(
        &master.csv,
        "v1",
        config.product_staging_encryption_key.as_ref(),
    )
    .expect("the generated Product Master must parse");
    assert_eq!(parsed.result.total_rows, 375);
    assert_eq!(parsed.result.valid_rows, 370);
    assert_eq!(parsed.result.malformed_rows, 5);
    assert_eq!(parsed.result.missing_assets.len(), 1);
    for code in [
        "invalidStableId",
        "duplicateStableId",
        "invalidFamily",
        "invalidLocale",
    ] {
        assert!(
            parsed.result.errors.iter().any(|error| error.code == code),
            "generated malformed rows must report {code}"
        );
    }

    let checksum = parsed.result.checksum.clone();
    let staged =
        stage_and_queue_product_import(&pool, parsed, "test", None, None, "syntheticScaleContract")
            .await
            .expect("synthetic Product Master staging transaction");
    assert!(staged.queued);
    assert!(!staged.result.reused);
    assert_eq!(staged.result.total_rows, 375);
    assert_eq!(staged.result.valid_rows, 370);
    assert_eq!(staged.result.malformed_rows, 5);

    let job = sqlx::query("SELECT status,payload FROM jobs WHERE id=$1")
        .bind(staged.operation_id)
        .fetch_one(&pool)
        .await
        .expect("durable Product import job");
    assert_eq!(job.try_get::<String, _>("status").unwrap(), "queued");
    assert_eq!(
        job.try_get::<Value, _>("payload").unwrap(),
        serde_json::json!({"importRunId": staged.operation_id}),
        "the job carries only the opaque operation id"
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
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(DISTINCT nonce) FROM product_import_private_staging WHERE import_run_id=$1",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        370,
        "every active synthetic source row must use an independent AES-GCM nonce"
    );
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
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_import_errors WHERE import_run_id=$1 AND severity='error'",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        5
    );
    let missing_asset = sqlx::query(
        r#"SELECT source_record_id,asset_type,source_reference
           FROM product_import_missing_assets WHERE import_run_id=$1"#,
    )
    .bind(staged.result.id)
    .fetch_one(&pool)
    .await
    .expect("one synthetic missing asset finding");
    assert_eq!(
        missing_asset
            .try_get::<String, _>("source_record_id")
            .unwrap(),
        master.first_stable_id
    );
    assert_eq!(
        missing_asset.try_get::<String, _>("asset_type").unwrap(),
        "image"
    );
    assert_eq!(
        missing_asset
            .try_get::<String, _>("source_reference")
            .unwrap(),
        master.missing_asset
    );

    let private = sqlx::query(
        r#"SELECT private.source_row_number,private.nonce,private.ciphertext,
                  private.authentication_tag,run.mapping_version,run.source_checksum
           FROM product_import_private_staging AS private
           JOIN product_import_runs AS run ON run.id=private.import_run_id
           WHERE private.import_run_id=$1 AND private.source_record_id=$2"#,
    )
    .bind(staged.result.id)
    .bind(&master.first_stable_id)
    .fetch_one(&pool)
    .await
    .expect("encrypted synthetic source row");
    let nonce: Vec<u8> = private.try_get("nonce").unwrap();
    let ciphertext: Vec<u8> = private.try_get("ciphertext").unwrap();
    let authentication_tag: Vec<u8> = private.try_get("authentication_tag").unwrap();
    assert!(
        !String::from_utf8_lossy(&ciphertext).contains(&master.first_private_price),
        "private price must not be stored as plaintext"
    );
    let mapping_version: String = private.try_get("mapping_version").unwrap();
    let stored_checksum: String = private.try_get("source_checksum").unwrap();
    let pricing = decrypt_private_pricing(
        config.product_staging_encryption_key.as_ref().unwrap(),
        PrivatePricingEnvelope {
            mapping_version: &mapping_version,
            checksum: &stored_checksum,
            source_row_number: private.try_get("source_row_number").unwrap(),
            stable_id: &master.first_stable_id,
            nonce: &nonce,
            ciphertext: &ciphertext,
            authentication_tag: &authentication_tag,
        },
    )
    .expect("row-bound AES-GCM source data decrypts through the pricing allow-list");
    assert_eq!(
        pricing.get("price").map(String::as_str),
        Some(master.first_private_price.as_str())
    );
    assert_eq!(pricing.get("currency").map(String::as_str), Some("TEST"));
    assert_eq!(pricing.len(), 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM product_import_normalized_records
               WHERE import_run_id=$1 AND normalized_payload::text LIKE '%' || $2 || '%'"#,
        )
        .bind(staged.result.id)
        .bind(&master.private_price_prefix)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "private prices must not enter normalized product facts"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM products WHERE product_import_run_id=$1",
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "request-time staging must not create products synchronously"
    );

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
    for column in ["job_status", "operation_status", "import_status"] {
        assert_eq!(lifecycle.try_get::<String, _>(column).unwrap(), "completed");
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM products
               WHERE product_import_run_id=$1 AND status='draft'
                 AND published_revision IS NULL AND indexable=false"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        370
    );
    let distribution = sqlx::query_as::<_, (String, String, i64)>(
        r#"SELECT family,payload->>'motorTechnology',COUNT(*)
           FROM products WHERE product_import_run_id=$1
           GROUP BY family,payload->>'motorTechnology'
           ORDER BY family,payload->>'motorTechnology'"#,
    )
    .bind(staged.result.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        distribution,
        vec![
            ("centrifugal".into(), "EC".into(), 185),
            ("inlineDuct".into(), "AC".into(), 185),
        ],
        "fan form and motor technology must remain independent facets"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM products
               WHERE product_import_run_id=$1 AND payload::text LIKE '%' || $2 || '%'"#,
        )
        .bind(staged.result.id)
        .bind(&master.private_price_prefix)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "private prices must not enter immutable product revisions"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM product_import_private_staging
               WHERE import_run_id=$1 AND status='promoted'"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        370
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM product_presentation_working AS presentation
               JOIN products AS product ON product.id=presentation.product_id
               WHERE product.product_import_run_id=$1
                 AND presentation.published_revision IS NULL
                 AND presentation.indexable=false"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        370
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM product_localizations AS localization
               JOIN products AS product ON product.id=localization.product_id
               WHERE product.product_import_run_id=$1"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "worker promotion creates drafts, never a public localization projection"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM published_products AS published
               JOIN products AS product ON product.id=published.id
               WHERE product.product_import_run_id=$1"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM public_routes AS route
               JOIN products AS product ON product.id=route.entity_id
               WHERE product.product_import_run_id=$1"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );

    let revision_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM product_revisions AS revision
           JOIN products AS product ON product.id=revision.product_id
           WHERE product.product_import_run_id=$1"#,
    )
    .bind(staged.result.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision_count, 370);
    let replay_parsed = parse_product_master(
        &master.csv,
        "v1",
        state.config.product_staging_encryption_key.as_ref(),
    )
    .expect("the same synthetic source re-parses");
    let replay = stage_and_queue_product_import(
        &pool,
        replay_parsed,
        "test",
        None,
        None,
        "syntheticScaleContract",
    )
    .await
    .expect("same-environment replay");
    assert!(replay.result.reused);
    assert!(!replay.queued);
    assert_eq!(replay.operation_id, staged.operation_id);
    assert_eq!(replay.result.status, "completed");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM product_import_runs
               WHERE environment='test' AND data_origin='verifiedCsv'
                 AND source_checksum=$1 AND mapping_version='v1'"#,
        )
        .bind(&checksum)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM product_revisions AS revision
               JOIN products AS product ON product.id=revision.product_id
               WHERE product.product_import_run_id=$1"#,
        )
        .bind(staged.result.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        revision_count,
        "an idempotent replay must not append product revisions"
    );
}
