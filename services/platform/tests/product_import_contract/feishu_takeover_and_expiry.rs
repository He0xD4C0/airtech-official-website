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
    support::assert_flyway_schema_current(&pool).await;

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
    support::assert_flyway_schema_current(&pool).await;

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
