use std::collections::BTreeMap;

use airtek_platform::{
    models::{FeishuSource, ProductFamily, ValidationIssue},
    services::feishu::{
        normalize_record, promote_record, rollback_sync_run, stage_invalid_record, FeishuRecord,
        FeishuTableMapping, MappedFeishuField, PromotionOutcome, StoredSourceAsset,
    },
};
use serde_json::{json, Map};
use sqlx::Row;
use uuid::Uuid;

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
    }
}

fn record(
    model: &str,
    modified: &str,
) -> airtek_platform::services::feishu::NormalizedFeishuRecord {
    normalize_record(
        &FeishuSource {
            wiki_token: "wiki".into(),
            table_id: "tblJjxOgBFL0FD0N".into(),
            name: "Axial Fans".into(),
            family: ProductFamily::Axial,
            application: None,
        },
        &mapping(),
        &FeishuRecord {
            record_id: "rec-integration".into(),
            fields: Map::from_iter([("型号".into(), json!(model))]),
            created_time: None,
            last_modified_time: Some(modified.into()),
        },
    )
}

async fn insert_run(pool: &sqlx::PgPool, connector_id: Uuid, run_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id,connector_id,source,dry_run,run_kind,mapping_version,status,resume_cursor,
            records_seen,records_valid,conflict_count,records_applied,records_failed,
            assets_seen,assets_copied,assets_reused,assets_failed,started_at,payload)
           VALUES ($1,$2,'feishu',false,'full','feishu-product-v1','validating',NULL,
                   0,0,0,0,0,0,0,0,0,now(),'{}'::jsonb)"#,
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

async fn finish_run(pool: &sqlx::PgPool, run_id: Uuid, status: &str) {
    sqlx::query("UPDATE sync_runs SET status=$2,completed_at=now() WHERE id=$1")
        .bind(run_id)
        .bind(status)
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
async fn automatic_publish_is_idempotent_preserves_cms_and_supports_safe_rollback() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let connector_id: Uuid =
        sqlx::query_scalar("SELECT connector_id FROM feishu_connector_settings LIMIT 1")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    let sources: Vec<FeishuSource> = serde_json::from_value(
        sqlx::query_scalar("SELECT sources FROM feishu_connector_settings WHERE connector_id=$1")
            .bind(connector_id)
            .fetch_one(&state.pool)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(sources.len(), 4);
    assert_eq!(sources[0].table_id, "tblzksABQBk6rdB6");
    assert_eq!(sources[0].family, ProductFamily::Centrifugal);
    assert_eq!(sources[1].table_id, "tblJjxOgBFL0FD0N");
    assert_eq!(sources[1].family, ProductFamily::Axial);
    assert_eq!(sources[2].table_id, "tblOtUU5MaxEnZI7");
    assert_eq!(sources[2].family, ProductFamily::Axial);
    assert_eq!(
        sources[2].application.as_deref(),
        Some("agriculture-livestock")
    );
    assert_eq!(sources[3].table_id, "tbl3hCDkvVIs2ZSi");
    assert_eq!(sources[3].family, ProductFamily::CrossFlow);

    let first_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, first_run).await;
    let first = record("AX-100", "1");
    let initial_slug = first.normalized_payload["slug"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(first.issues.is_empty(), "{:?}", first.issues);
    assert_eq!(
        promote_record(
            &state,
            connector_id,
            first_run,
            first_run,
            "feishu-product-v1",
            &format!("feishu-run:{first_run}"),
            2,
            &first,
            &[],
        )
        .await
        .unwrap(),
        PromotionOutcome::Created
    );

    let product = sqlx::query(
        "SELECT id,current_revision,published_revision FROM products WHERE stable_id=$1",
    )
    .bind(&first.source_record_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    let product_id: Uuid = product.try_get("id").unwrap();
    assert_eq!(product.try_get::<i64, _>("current_revision").unwrap(), 1);
    assert_eq!(product.try_get::<i64, _>("published_revision").unwrap(), 1);
    finish_run(&state.pool, first_run, "completed").await;

    let repeat_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, repeat_run).await;
    let unchanged = record("AX-100", "2");
    assert_eq!(
        promote_record(
            &state,
            connector_id,
            repeat_run,
            repeat_run,
            "feishu-product-v1",
            &format!("feishu-run:{repeat_run}"),
            2,
            &unchanged,
            &[],
        )
        .await
        .unwrap(),
        PromotionOutcome::Unchanged
    );
    let revision_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM product_revisions WHERE product_id=$1")
            .bind(product_id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(revision_count, 1);
    finish_run(&state.pool, repeat_run, "completed").await;

    let content = json!({"sortOrder": 9, "relatedContentIds": [], "mediaGallery": []});
    let seo = json!({
        "title": "Curated SEO", "description": null,
        "canonicalPath": "/en/products/axial/cms-axial-product", "indexable": true
    });
    let mut presentation = state.pool.begin().await.unwrap();
    sqlx::query(
        r#"INSERT INTO product_presentation_revisions
           (product_id,locale,revision,source_product_revision,slug,title,summary,
            content,seo_metadata,translation_state,is_placeholder,indexable,data_origin,
            created_by,created_at)
           VALUES ($1,'en',2,1,'cms-axial-product','CMS curated title','CMS summary',
                   $2,$3,'verified',false,true,'editorial','cms@example.test',now())"#,
    )
    .bind(product_id)
    .bind(&content)
    .bind(&seo)
    .execute(&mut *presentation)
    .await
    .unwrap();
    sqlx::query(
        r#"UPDATE product_presentation_working
           SET current_revision=2,slug='cms-axial-product',title='CMS curated title',
               summary='CMS summary',content=$2,seo_metadata=$3,updated_by='cms@example.test'
           WHERE product_id=$1 AND locale='en'"#,
    )
    .bind(product_id)
    .bind(&content)
    .bind(&seo)
    .execute(&mut *presentation)
    .await
    .unwrap();
    presentation.commit().await.unwrap();

    let source_asset_id = Uuid::new_v4();
    let checksum = "b".repeat(64);
    sqlx::query(
        r#"INSERT INTO media_assets
           (id,storage_key,public_url,original_name,media_type,byte_size,checksum,
            metadata,created_at,storage_backend,content_type,uploaded_by)
           VALUES ($1,$2,$3,'AX-200-drawing.pdf','application/pdf',25,$4,
                   '{}'::jsonb,now(),'local','application/pdf','feishuSync')"#,
    )
    .bind(source_asset_id)
    .bind(format!("media/feishu/{source_asset_id}.pdf"))
    .bind(format!("https://media.example.test/{source_asset_id}.pdf"))
    .bind(&checksum)
    .execute(&state.pool)
    .await
    .unwrap();
    let source_assets = vec![StoredSourceAsset {
        media_asset_id: source_asset_id,
        storage_key: format!("media/feishu/{source_asset_id}.pdf"),
        preview_storage_key: None,
        checksum: checksum.clone(),
        media_type: "application/pdf".into(),
        byte_size: 25,
        original_name: "AX-200-drawing.pdf".into(),
        source_field_id: "fld-drawing".into(),
        source_field_name: "产品图纸".into(),
        usage: "drawing".into(),
        newly_created: false,
    }];

    let update_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, update_run).await;
    let changed = record("AX-200", "3");
    assert_eq!(
        promote_record(
            &state,
            connector_id,
            update_run,
            update_run,
            "feishu-product-v1",
            &format!("feishu-run:{update_run}"),
            2,
            &changed,
            &source_assets,
        )
        .await
        .unwrap(),
        PromotionOutcome::Updated
    );
    let published = sqlx::query(
        r#"SELECT product.current_revision,product.published_revision,localization.slug,
                  localization.title,(localization.content->>'sortOrder')::int AS sort_order
           FROM products product JOIN product_localizations localization
             ON localization.product_id=product.id
            AND localization.product_revision=product.published_revision
           WHERE product.id=$1 AND localization.locale='en'"#,
    )
    .bind(product_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(published.try_get::<i64, _>("current_revision").unwrap(), 2);
    assert_eq!(
        published.try_get::<i64, _>("published_revision").unwrap(),
        2
    );
    assert_eq!(
        published.try_get::<String, _>("slug").unwrap(),
        "cms-axial-product"
    );
    assert_eq!(
        published.try_get::<String, _>("title").unwrap(),
        "CMS curated title"
    );
    assert_eq!(published.try_get::<i32, _>("sort_order").unwrap(), 9);
    let public = build_router(state.clone());
    let response = public
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products/cms-axial-product/assets?family=axial")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["productRevision"], 2);
    assert_eq!(body["items"][0]["assetId"], source_asset_id.to_string());
    assert_eq!(body["items"][0]["usage"], "drawing");
    assert_eq!(body["items"][0]["sha256"], checksum);
    assert!(body["items"][0]["previewUrl"].is_null());
    assert_eq!(
        body["items"][0]["downloadUrl"],
        format!("/api/public/v1/media/{source_asset_id}/download")
    );
    finish_run(&state.pool, update_run, "completed").await;

    let rejected_run = Uuid::new_v4();
    insert_run(&state.pool, connector_id, rejected_run).await;
    let mut invalid = record("AX-invalid", "4");
    invalid.issues.push(ValidationIssue {
        field_path: "model".into(),
        code: "testInvalid".into(),
        detail: "Synthetic invalid source record.".into(),
    });
    stage_invalid_record(
        &state,
        connector_id,
        rejected_run,
        rejected_run,
        "feishu-product-v1",
        &format!("feishu-run:{rejected_run}"),
        2,
        &invalid,
    )
    .await
    .unwrap();
    let after_invalid: (i64, Option<i64>) =
        sqlx::query_as("SELECT current_revision,published_revision FROM products WHERE id=$1")
            .bind(product_id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(after_invalid, (2, Some(2)));
    finish_run(&state.pool, rejected_run, "completedWithErrors").await;
    let rollback = rollback_sync_run(&state, update_run, "integration@example.test")
        .await
        .unwrap();
    assert_eq!(rollback.restored, 1);
    assert_eq!(rollback.skipped, 0);
    let rolled_back: (i64, Option<i64>) =
        sqlx::query_as("SELECT current_revision,published_revision FROM products WHERE id=$1")
            .bind(product_id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(rolled_back, (2, Some(1)));
    let stale_route = public
        .clone()
        .oneshot(
            Request::get("/api/public/v1/products/cms-axial-product/assets?family=axial")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_route.status(), StatusCode::NOT_FOUND);
    let response = public
        .oneshot(
            Request::get(format!(
                "/api/public/v1/products/{initial_slug}/assets?family=axial"
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["productRevision"], 1);
    assert_eq!(body["items"], json!([]));

    sandbox.cleanup().await;
}
