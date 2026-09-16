use std::collections::HashSet;

use airtek_platform::services::admin_product_query::{list_admin_products, ProductListFilter};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::json;
use uuid::Uuid;

use super::support;

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn five_thousand_products_use_filtered_faceted_keyset_pages() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool();
    let connector_id = Uuid::new_v4();
    let run_id = Uuid::new_v4();
    let snapshot_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO source_connectors(id,connector_type,display_name,enabled) VALUES($1,'feishu','TEST product paging',false)",
    )
    .bind(connector_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,source,dry_run,mapping_version,status,started_at,payload)
           VALUES($1,'feishu',true,'test','completed',now(),
                  jsonb_build_object('id',$1::text,'source','feishu','dryRun',true,
                    'mappingVersion','test','status','completed','resumeCursor',NULL,
                    'recordsSeen',0,'recordsValid',0,'conflictCount',0,
                    'startedAt',now(),'completedAt',now(),'error',NULL))"#,
    )
    .bind(run_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO source_snapshots
           (id,connector_id,sync_run_id,source_record_id,source_revision,checksum,source_payload)
           VALUES($1,$2,$3,'TEST-PAGE-SOURCE','1','test','{}')"#,
    )
    .bind(snapshot_id)
    .bind(connector_id)
    .bind(run_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO products
           (id,stable_id,model,slug,locale,family,source_snapshot_id,source_revision,
            status,current_revision,published_revision,indexable,payload,updated_at,data_origin)
           SELECT id,stable_id,model,slug,'en','axial',$1,'1','draft',1,NULL,false,
                  jsonb_build_object(
                    'id',id::text,'stableId',stable_id,'model',model,'slug',slug,
                    'locale','en','family','axial','subtype',NULL,'motorTechnology',NULL,
                    'title',format('Synthetic %s',number),'summary',NULL,
                    'seo',jsonb_build_object('title',NULL,'description',NULL,'canonicalPath',NULL,'indexable',false),
                    'sortOrder',0,'relatedContentIds',jsonb_build_array(),
                    'specifications',jsonb_build_array(),'performanceCurves',jsonb_build_array(),
                    'sourceSnapshotId',$1::text,'sourceRevision','1','currentRevision',1,
                    'publishedRevision',NULL,'status','draft','indexable',false,'updatedAt',now()),
                  now(),'feishu'
           FROM (
             SELECT number,gen_random_uuid() AS id,
                    'SYNTH-'||lpad(number::text,5,'0') AS stable_id,
                    'MODEL-'||lpad(number::text,5,'0') AS model,
                    'synth-'||number::text AS slug
             FROM generate_series(1,5000) number
           ) generated"#,
    )
    .bind(snapshot_id)
    .execute(pool)
    .await
    .unwrap();

    let state = super::postgres_state(sandbox.connection_url());
    let anchor_id =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM products WHERE stable_id='SYNTH-00050'")
            .fetch_one(sandbox.pool())
            .await
            .unwrap();
    let scope = format!(
        "admin.products|{:?}|{:?}|{:?}|{:?}",
        Some("synth"),
        Some("axial"),
        Some("draft"),
        Some("verified")
    );
    let legacy_cursor = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "version": 1,
            "scope": scope,
            "position": anchor_id,
        }))
        .unwrap(),
    );
    let legacy_page = list_admin_products(
        &state,
        ProductListFilter {
            cursor: Some(legacy_cursor),
            limit: Some(2),
            search: Some("synth".into()),
            family: Some("axial".into()),
            status: Some("draft".into()),
            data_state: Some("verified".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(legacy_page.items[0].stable_id, "SYNTH-00051");
    assert_eq!(legacy_page.items[1].stable_id, "SYNTH-00052");

    let wrong_scope =
        "admin.products|Some(\"synth\")|Some(\"centrifugal\")|Some(\"draft\")|Some(\"verified\")";
    let filtered_cursor = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "version": 1,
            "scope": wrong_scope,
            "position": anchor_id,
        }))
        .unwrap(),
    );
    assert!(list_admin_products(
        &state,
        ProductListFilter {
            cursor: Some(filtered_cursor),
            limit: Some(2),
            search: Some("synth".into()),
            family: Some("centrifugal".into()),
            status: Some("draft".into()),
            data_state: Some("verified".into()),
        },
    )
    .await
    .is_err());

    let missing_cursor = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&json!({
            "version": 1,
            "scope": scope,
            "position": Uuid::new_v4(),
        }))
        .unwrap(),
    );
    assert!(list_admin_products(
        &state,
        ProductListFilter {
            cursor: Some(missing_cursor),
            limit: Some(2),
            search: Some("synth".into()),
            family: Some("axial".into()),
            status: Some("draft".into()),
            data_state: Some("verified".into()),
        },
    )
    .await
    .is_err());

    let mut cursor = None;
    let mut ids = HashSet::new();
    loop {
        let page = list_admin_products(
            &state,
            ProductListFilter {
                cursor,
                limit: Some(100),
                search: Some("synth".into()),
                family: Some("axial".into()),
                status: Some("draft".into()),
                data_state: Some("verified".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 5000);
        assert!(page.items.iter().all(|item| ids.insert(item.id)));
        assert_eq!(page.family_counts[0].count, 5000);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 5000);
    drop(state);
    sandbox.cleanup().await;
}
