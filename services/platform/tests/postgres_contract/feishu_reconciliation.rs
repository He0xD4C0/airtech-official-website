use std::collections::BTreeMap;

use airtek_platform::{
    models::{FeishuSource, ProductFamily, UpdateFeishuSettings},
    services::feishu::{
        execute_source_purge, get_feishu_settings, normalize_record, promote_record,
        reconcile_missing_records, update_feishu_settings, FeishuRecord, FeishuTableMapping,
        MappedFeishuField, PromotionOutcome,
    },
};
use serde_json::{json, Map};

use super::*;

fn mapping() -> FeishuTableMapping {
    FeishuTableMapping {
        fields: BTreeMap::from([(
            "model".into(),
            MappedFeishuField {
                id: "fld-model".into(),
                name: "型号".into(),
                field_type: 1,
            },
        )]),
        attachments: vec![],
        private_fields: vec![],
    }
}

fn record(
    source: &FeishuSource,
    modified: &str,
) -> airtek_platform::services::feishu::NormalizedFeishuRecord {
    normalize_record(
        source,
        &mapping(),
        &FeishuRecord {
            record_id: "rec-delete-and-return".into(),
            fields: Map::from_iter([("型号".into(), json!("AX-DELETE-RETURN"))]),
            created_time: None,
            last_modified_time: Some(modified.into()),
        },
    )
}

async fn insert_run(pool: &sqlx::PgPool, connector_id: Uuid, run_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,connector_id,source,dry_run,mapping_version,status,started_at,payload)
           VALUES ($1,$2,'feishu',false,'feishu-product-v1','validating',now(),'{}')"#,
    )
    .bind(run_id)
    .bind(connector_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO product_import_runs
           (id,sync_run_id,connector_id,environment,data_origin,dry_run,status,
            mapping_version,source_checksum,records_received,records_valid,error_count,
            started_at,created_at)
           VALUES ($1,$1,$2,'development','feishu',false,'validating',
                   'feishu-product-v1',$3,0,0,0,now(),now())"#,
    )
    .bind(run_id)
    .bind(connector_id)
    .bind(format!("feishu-run:{run_id}"))
    .execute(pool)
    .await
    .unwrap();
}

async fn finish_run(pool: &sqlx::PgPool, run_id: Uuid) {
    sqlx::query("UPDATE sync_runs SET status='completed',completed_at=now() WHERE id=$1")
        .bind(run_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE product_import_runs SET status='completed',completed_at=now() WHERE id=$1")
        .bind(run_id)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn missing_records_hide_then_purge_and_reappearance_gets_a_new_product_id() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let connector_id: Uuid =
        sqlx::query_scalar("SELECT connector_id FROM feishu_connector_settings LIMIT 1")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    let source = FeishuSource {
        enabled: true,
        wiki_token: "wiki-delete".into(),
        table_id: "table-delete".into(),
        name: "Deletion reconciliation".into(),
        family: ProductFamily::Axial,
        application: None,
    };

    let create_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, create_run).await;
    let initial = record(&source, "1");
    assert_eq!(
        promote_record(
            &state,
            connector_id,
            create_run,
            create_run,
            "feishu-product-v1",
            &format!("feishu-run:{create_run}"),
            2,
            &initial,
            &[],
        )
        .await
        .unwrap(),
        PromotionOutcome::Created
    );
    let original_id: Uuid = sqlx::query_scalar("SELECT id FROM products WHERE stable_id=$1")
        .bind(&initial.source_record_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    finish_run(&state.pool, create_run).await;

    let missing_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, missing_run).await;
    assert_eq!(
        reconcile_missing_records(
            &state,
            missing_run,
            connector_id,
            &source.wiki_token,
            &source.table_id,
        )
        .await
        .unwrap(),
        1
    );
    let hidden: (String, Option<i64>, bool, String) = sqlx::query_as(
        r#"SELECT product.status,product.published_revision,product.indexable,ownership.state
           FROM products product JOIN feishu_product_ownership ownership
             ON ownership.product_id=product.id WHERE product.id=$1"#,
    )
    .bind(original_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(
        hidden,
        ("archived".into(), None, false, "pendingDelete".into())
    );
    let route_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public_routes WHERE entity_type='product' AND entity_id=$1",
    )
    .bind(original_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(route_count, 0);
    finish_run(&state.pool, missing_run).await;

    let return_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, return_run).await;
    let returned = record(&source, "2");
    assert_eq!(
        promote_record(
            &state,
            connector_id,
            return_run,
            return_run,
            "feishu-product-v1",
            &format!("feishu-run:{return_run}"),
            2,
            &returned,
            &[],
        )
        .await
        .unwrap(),
        PromotionOutcome::Updated
    );
    finish_run(&state.pool, return_run).await;
    let stale_purge =
        execute_source_purge(&state, connector_id, &source.wiki_token, &source.table_id)
            .await
            .unwrap();
    assert_eq!(stale_purge["deleted"], 0);
    let active: (Uuid, String, Option<i64>) = sqlx::query_as(
        r#"SELECT product.id,ownership.state,product.published_revision
           FROM products product JOIN feishu_product_ownership ownership
             ON ownership.product_id=product.id WHERE product.stable_id=$1"#,
    )
    .bind(&returned.source_record_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(active.0, original_id);
    assert_eq!(active.1, "active");
    assert!(active.2.is_some());

    let second_missing_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, second_missing_run).await;
    assert_eq!(
        reconcile_missing_records(
            &state,
            second_missing_run,
            connector_id,
            &source.wiki_token,
            &source.table_id,
        )
        .await
        .unwrap(),
        1
    );
    finish_run(&state.pool, second_missing_run).await;
    let purge = execute_source_purge(&state, connector_id, &source.wiki_token, &source.table_id)
        .await
        .unwrap();
    assert_eq!(purge["deleted"], 1);
    let removed: i64 = sqlx::query_scalar("SELECT count(*) FROM products WHERE id=$1")
        .bind(original_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(removed, 0);

    let recreate_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, recreate_run).await;
    let recreated = record(&source, "3");
    assert_eq!(
        promote_record(
            &state,
            connector_id,
            recreate_run,
            recreate_run,
            "feishu-product-v1",
            &format!("feishu-run:{recreate_run}"),
            2,
            &recreated,
            &[],
        )
        .await
        .unwrap(),
        PromotionOutcome::Created
    );
    let replacement_id: Uuid = sqlx::query_scalar("SELECT id FROM products WHERE stable_id=$1")
        .bind(&recreated.source_record_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_ne!(replacement_id, original_id);
    finish_run(&state.pool, recreate_run).await;
    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn disabling_a_source_immediately_hides_and_queues_its_products_for_purge() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let before = get_feishu_settings(&state).await.unwrap();
    let source = before.sources[0].clone();
    let create_run = Uuid::new_v4();
    insert_run(&state.pool, before.connector_id, create_run).await;
    let product = record(&source, "1");
    promote_record(
        &state,
        before.connector_id,
        create_run,
        create_run,
        "feishu-product-v1",
        &format!("feishu-run:{create_run}"),
        2,
        &product,
        &[],
    )
    .await
    .unwrap();
    let product_id: Uuid = sqlx::query_scalar("SELECT id FROM products WHERE stable_id=$1")
        .bind(&product.source_record_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    finish_run(&state.pool, create_run).await;

    let mut sources = before.sources.clone();
    sources[0].enabled = false;
    let update = UpdateFeishuSettings {
        app_id: String::new(),
        app_secret: String::new(),
        clear_credentials: true,
        sources,
        enabled: false,
        interval_enabled: false,
        interval_minutes: 15,
        daily_enabled: false,
        daily_local_time: "02:00".into(),
    };
    update_feishu_settings(
        &state,
        before.revision,
        &update,
        "settings-test@example.com",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let state_and_publication: (String, Option<i64>) = sqlx::query_as(
        r#"SELECT ownership.state,product.published_revision
           FROM feishu_product_ownership ownership
           JOIN products product ON product.id=ownership.product_id
           WHERE product.id=$1"#,
    )
    .bind(product_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(state_and_publication, ("pendingDelete".into(), None));
    let purge_jobs: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM jobs WHERE job_type='feishuSourcePurge'
           AND payload->>'wikiToken'=$1 AND payload->>'tableId'=$2"#,
    )
    .bind(&source.wiki_token)
    .bind(&source.table_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(purge_jobs, 1);

    execute_source_purge(
        &state,
        before.connector_id,
        &source.wiki_token,
        &source.table_id,
    )
    .await
    .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM products WHERE id=$1")
        .bind(product_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    sandbox.cleanup().await;
}
