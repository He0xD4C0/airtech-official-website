use airtek_platform::{
    models::{
        CurvePoint, FactState, PerformanceCurve, Product, ProductFamily, PublicationStatus,
        SeoMetadata, SpecValue,
    },
    services::product_import::{
        decrypt_private_pricing, parse_product_master, promote_staged_product_import,
        reset_staged_product_import_for_retry, stage_and_queue_product_import,
        PrivatePricingEnvelope,
    },
    worker::run_one_pending_job,
    AppState, Config,
};
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, Row};
use uuid::Uuid;

struct SyntheticProductMaster {
    csv: String,
    first_stable_id: String,
    first_private_price: String,
    private_price_prefix: String,
    missing_asset: String,
}

fn synthetic_product_master(prefix: &str) -> SyntheticProductMaster {
    let mut csv =
        String::from("stable_id,model,family,motor_technology,title,locale,price,currency,image\n");
    let private_price_prefix = format!("ci-private-{prefix}-");
    let missing_asset = format!("ci-missing-{prefix}.svg");
    let mut first_stable_id = String::new();
    let mut first_private_price = String::new();

    for index in 0..370 {
        let stable_id = format!("CI-{prefix}-{index:03}");
        let model = format!("CI-MODEL-{prefix}-{index:03}");
        let (family, motor_technology) = if index % 2 == 0 {
            ("Centrifugal", "EC")
        } else {
            ("Inline Duct", "AC")
        };
        let private_price = format!("{private_price_prefix}{index:03}");
        let image = if index == 0 {
            missing_asset.as_str()
        } else {
            ""
        };
        csv.push_str(&format!(
            "{stable_id},{model},{family},{motor_technology},CI synthetic product {index:03},en,{private_price},TEST,{image}\n"
        ));
        if index == 0 {
            first_stable_id = stable_id;
            first_private_price = private_price;
        }
    }

    // Each malformed source row has exactly one deliberate fatal error. These
    // rows exercise reporting without borrowing any value from the controlled
    // commercial Product Master acceptance fixture above.
    csv.push_str(&format!(
        ",CI-INVALID-EMPTY-{prefix},Centrifugal,EC,CI invalid empty stable id,en,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "{first_stable_id},CI-INVALID-DUP-{prefix},Centrifugal,EC,CI invalid duplicate stable id,en,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "CI-{prefix}-BAD-FAMILY,CI-INVALID-FAMILY-{prefix},Unsupported,EC,CI invalid family,en,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "CI-{prefix}-BAD-LOCALE,CI-INVALID-LOCALE-{prefix},Centrifugal,EC,CI invalid locale,en_US,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "CI-{prefix}-NO-FAMILY,CI-INVALID-NO-FAMILY-{prefix},,AC,CI invalid empty family,en,ci-invalid,TEST,\n"
    ));

    SyntheticProductMaster {
        csv,
        first_stable_id,
        first_private_price,
        private_price_prefix,
        missing_asset,
    }
}

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
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");

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
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");
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
    // job/operation state reset in airtekctl.
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

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn feishu_takes_over_verified_csv_stable_id_and_projects_all_facts_atomically() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");

    let suffix = Uuid::new_v4().simple().to_string();
    let stable_id = format!("CI-TAKEOVER-{suffix}");
    let csv = format!(
        "stable_id,model,family,title,frequency,voltage,power,power_unit\n{stable_id},CSV-{suffix},Centrifugal,CSV authority,50,230,100,W\n"
    );
    let mut config = Config::for_test();
    config.database_url = Some(database_url.clone());
    let parsed = parse_product_master(
        &csv,
        "airtek-basic-v1",
        config.product_staging_encryption_key.as_ref(),
    )
    .expect("synthetic verified CSV");
    let staged =
        stage_and_queue_product_import(&pool, parsed, "test", None, None, "takeoverContract")
            .await
            .expect("CSV staging");
    promote_staged_product_import(&pool, staged.operation_id, None)
        .await
        .expect("CSV promotion");
    let csv_product = sqlx::query(
        "SELECT id,current_revision,data_origin,product_import_run_id FROM products WHERE stable_id=$1",
    )
    .bind(&stable_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let canonical_id: Uuid = csv_product.try_get("id").unwrap();
    assert_eq!(
        csv_product.try_get::<String, _>("data_origin").unwrap(),
        "verifiedCsv"
    );
    assert_eq!(
        csv_product.try_get::<i64, _>("current_revision").unwrap(),
        1
    );
    assert_eq!(
        csv_product
            .try_get::<Option<Uuid>, _>("product_import_run_id")
            .unwrap(),
        Some(staged.operation_id)
    );
    assert!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_specs WHERE product_id=$1 AND product_revision=1",
        )
        .bind(canonical_id)
        .fetch_one(&pool)
        .await
        .unwrap()
            > 0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM product_operating_conditions WHERE product_id=$1 AND product_revision=1",
        )
        .bind(canonical_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );

    let now = chrono::Utc::now();
    let connector_id = Uuid::new_v4();
    let sync_run_id = Uuid::new_v4();
    let snapshot_id = Uuid::new_v4();
    let source_revision = format!("feishu-revision-{suffix}");
    let incoming = Product {
        id: Uuid::new_v4(),
        stable_id: stable_id.clone(),
        model: Some(format!("FEISHU-{suffix}")),
        slug: format!("feishu-{suffix}"),
        locale: "en".into(),
        family: ProductFamily::Centrifugal,
        subtype: Some("plugFan".into()),
        motor_technology: Some("EC".into()),
        title: "Feishu authority".into(),
        summary: None,
        seo: SeoMetadata::default(),
        sort_order: 0,
        related_content_ids: Vec::new(),
        specifications: vec![SpecValue {
            key: "power".into(),
            label: "Power".into(),
            value: Some(serde_json::json!(125)),
            unit: Some("W".into()),
            operating_condition: Some("230 V / 50 Hz".into()),
            state: FactState::Verified,
            source_reference: Some(format!("feishu:{source_revision}")),
        }],
        performance_curves: vec![PerformanceCurve {
            airflow_unit: "m³/h".into(),
            pressure_unit: "Pa".into(),
            speed_rpm: Some(1_400),
            density_kg_m3: Some(1.2),
            voltage: Some("230 V".into()),
            test_method: Some("AMCA 210".into()),
            source_reference: format!("feishu:{source_revision}"),
            state: FactState::Verified,
            points: vec![
                CurvePoint {
                    airflow: 100.0,
                    pressure: 250.0,
                },
                CurvePoint {
                    airflow: 200.0,
                    pressure: 180.0,
                },
            ],
        }],
        source_snapshot_id: snapshot_id,
        source_revision: source_revision.clone(),
        current_revision: 1,
        published_revision: None,
        status: PublicationStatus::Draft,
        indexable: false,
        updated_at: now,
    };
    let mut staging_payload = serde_json::to_value(&incoming).unwrap();
    let staging_object = staging_payload.as_object_mut().unwrap();
    staging_object.insert(
        "operatingConditions".into(),
        serde_json::json!([{
            "key": "50-hz",
            "label": "50 Hz",
            "frequencyHz": 50,
            "voltage": "230 V",
            "state": "verified",
            "sourceReference": format!("feishu:{source_revision}")
        }]),
    );
    staging_object.insert(
        "assets".into(),
        serde_json::json!([{
            "assetType": "datasheet",
            "locale": "en",
            "revision": source_revision,
            "storageKey": format!("contract/{suffix}/datasheet.pdf"),
            "checksum": "test-only-clean-asset-checksum",
            "scanStatus": "clean",
            "accessLevel": "public",
            "sourceReference": format!("feishu:{source_revision}")
        }]),
    );
    sqlx::query(
        "INSERT INTO source_connectors (id,connector_type,display_name,enabled) VALUES ($1,'feishu',$2,true)",
    )
    .bind(connector_id)
    .bind(format!("CI takeover {suffix}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,source,dry_run,mapping_version,status,records_seen,records_valid,
            conflict_count,started_at,payload)
           VALUES ($1,'feishu',false,'contract-v1','readyToPublish',1,1,0,$2,$3)"#,
    )
    .bind(sync_run_id)
    .bind(now)
    .bind(serde_json::json!({"id": sync_run_id, "source": "feishu"}))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO source_snapshots
           (id,connector_id,sync_run_id,source_record_id,source_revision,checksum,
            source_payload,received_at)
           VALUES ($1,$2,$3,$4,$5,'contract-feishu-checksum',$6,$7)"#,
    )
    .bind(snapshot_id)
    .bind(connector_id)
    .bind(sync_run_id)
    .bind(&stable_id)
    .bind(&source_revision)
    .bind(&staging_payload)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO staging_records
           (id,sync_run_id,source_snapshot_id,source_record_id,validation_status,
            normalized_payload,validation_errors,created_at)
           VALUES ($1,$2,$3,$4,'valid',$5,'[]'::jsonb,$6)"#,
    )
    .bind(Uuid::new_v4())
    .bind(sync_run_id)
    .bind(snapshot_id)
    .bind(&stable_id)
    .bind(&staging_payload)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    AppState::new(config)
        .unwrap()
        .persist_product(&incoming)
        .await
        .expect("Feishu stable_id takeover");

    let current = sqlx::query(
        "SELECT id,current_revision,data_origin,product_import_run_id FROM products WHERE stable_id=$1",
    )
    .bind(&stable_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(current.try_get::<Uuid, _>("id").unwrap(), canonical_id);
    assert_eq!(current.try_get::<i64, _>("current_revision").unwrap(), 2);
    assert_eq!(
        current.try_get::<String, _>("data_origin").unwrap(),
        "feishu"
    );
    assert_eq!(
        current
            .try_get::<Option<Uuid>, _>("product_import_run_id")
            .unwrap(),
        None
    );
    let origins = sqlx::query(
        "SELECT revision,data_origin,product_import_run_id FROM product_revisions WHERE product_id=$1 ORDER BY revision",
    )
    .bind(canonical_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(origins.len(), 2);
    assert_eq!(
        origins[0].try_get::<String, _>("data_origin").unwrap(),
        "verifiedCsv"
    );
    assert_eq!(
        origins[0]
            .try_get::<Option<Uuid>, _>("product_import_run_id")
            .unwrap(),
        Some(staged.operation_id)
    );
    assert_eq!(
        origins[1].try_get::<String, _>("data_origin").unwrap(),
        "feishu"
    );
    assert_eq!(
        origins[1]
            .try_get::<Option<Uuid>, _>("product_import_run_id")
            .unwrap(),
        None
    );
    for (table, expected) in [
        ("product_specs", 1_i64),
        ("product_operating_conditions", 1_i64),
        ("performance_curves", 1_i64),
    ] {
        let count = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM {table} WHERE product_id=$1 AND product_revision=2"
        ))
        .bind(canonical_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, expected,
            "{table} must be projected in the takeover transaction"
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM product_assets WHERE product_id=$1")
            .bind(canonical_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn expired_private_staging_requires_atomic_cryptographic_erasure() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("platform migrations");

    let run_id = Uuid::new_v4();
    let staging_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO product_import_runs
               (id,data_origin,dry_run,status,mapping_version,source_checksum,
                created_at,environment)
           VALUES ($1,'verifiedCsv',false,'failed','zeroization-contract-v1',$2,
                   now(),'test')"#,
    )
    .bind(run_id)
    .bind(format!("zeroization-contract-{run_id}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO product_import_private_staging
               (id,import_run_id,source_record_id,source_row_number,ciphertext,
                encryption_algorithm,encryption_key_id,nonce,authentication_tag,
                checksum,status,created_at,expires_at)
           VALUES ($1,$2,'TEST-ZEROIZATION',2,decode('010203','hex'),
                   'AES-256-GCM',$3,decode(repeat('ab',12),'hex'),
                   decode(repeat('cd',16),'hex'),'test-checksum','rejected',
                   now(),now() + interval '1 day')"#,
    )
    .bind(staging_id)
    .bind(run_id)
    .bind(format!("zeroization-contract-key-{run_id}"))
    .execute(&pool)
    .await
    .unwrap();

    let status_only_error =
        sqlx::query("UPDATE product_import_private_staging SET status='expired' WHERE id=$1")
            .bind(staging_id)
            .execute(&pool)
            .await
            .expect_err("expired status cannot retain decryptable private staging bytes");
    assert_eq!(
        status_only_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );

    sqlx::query(
        r#"UPDATE product_import_private_staging
           SET status='expired',
               ciphertext=decode(repeat('00',octet_length(ciphertext)),'hex'),
               nonce=decode(repeat('00',octet_length(nonce)),'hex'),
               authentication_tag=decode(repeat('00',octet_length(authentication_tag)),'hex'),
               processed_at=now()
           WHERE id=$1"#,
    )
    .bind(staging_id)
    .execute(&pool)
    .await
    .expect("worker-compatible atomic zeroization");
    let zeroized = sqlx::query(
        "SELECT status,ciphertext,nonce,authentication_tag FROM product_import_private_staging WHERE id=$1",
    )
    .bind(staging_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(zeroized.try_get::<String, _>("status").unwrap(), "expired");
    for column in ["ciphertext", "nonce", "authentication_tag"] {
        assert!(zeroized
            .try_get::<Vec<u8>, _>(column)
            .unwrap()
            .iter()
            .all(|byte| *byte == 0));
    }

    sqlx::query("DELETE FROM product_import_runs WHERE id=$1")
        .bind(run_id)
        .execute(&pool)
        .await
        .unwrap();
}
