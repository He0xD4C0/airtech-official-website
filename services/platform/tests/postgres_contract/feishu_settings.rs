use airtek_platform::{
    models::{FeishuSettings, FeishuSyncTrigger, UpdateFeishuSettings},
    services::feishu::{
        get_feishu_settings, queue_full_sync, try_queue_full_sync, update_feishu_settings,
    },
};
use axum::response::IntoResponse;

use super::*;

fn update(settings: &FeishuSettings, app_id: &str, app_secret: &str) -> UpdateFeishuSettings {
    UpdateFeishuSettings {
        app_id: app_id.into(),
        app_secret: app_secret.into(),
        clear_credentials: false,
        sources: settings.sources.clone(),
        enabled: false,
        interval_enabled: false,
        interval_minutes: 15,
        daily_enabled: false,
        daily_local_time: "02:00".into(),
    }
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn gui_credentials_are_plaintext_write_only_rotatable_and_clearable() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let before = get_feishu_settings(&state).await.unwrap();
    let partial_id = sqlx::query(
        "UPDATE feishu_connector_settings SET app_id='partial',app_secret=NULL WHERE connector_id=$1",
    )
    .bind(before.connector_id)
    .execute(&state.pool)
    .await;
    assert!(partial_id.is_err());
    let partial_secret = sqlx::query(
        "UPDATE feishu_connector_settings SET app_id=NULL,app_secret='partial' WHERE connector_id=$1",
    )
    .bind(before.connector_id)
    .execute(&state.pool)
    .await;
    assert!(partial_secret.is_err());
    let first_secret = format!("first-secret-{}", Uuid::new_v4());
    let first = update_feishu_settings(
        &state,
        before.revision,
        &update(&before, "cli_plaintext", &first_secret),
        "settings-test@example.com",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    assert_eq!(first.app_id.as_deref(), Some("cli_plaintext"));
    assert!(first.secret_configured);
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT app_secret FROM feishu_connector_settings WHERE connector_id=$1",
    )
    .bind(first.connector_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(stored.as_deref(), Some(first_secret.as_str()));
    let projection = serde_json::to_string(&first).unwrap();
    assert!(!projection.contains(&first_secret));
    assert!(!projection.contains("appSecret"));
    let audit: String = sqlx::query_scalar(
        r#"SELECT concat(before_value::text,after_value::text) FROM audit_log
           WHERE action='settings.feishu.update' ORDER BY occurred_at DESC LIMIT 1"#,
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert!(!audit.contains(&first_secret));

    let retained = update_feishu_settings(
        &state,
        first.revision,
        &update(&first, "cli_plaintext", ""),
        "settings-test@example.com",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let retained_secret: Option<String> = sqlx::query_scalar(
        "SELECT app_secret FROM feishu_connector_settings WHERE connector_id=$1",
    )
    .bind(first.connector_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(retained_secret.as_deref(), Some(first_secret.as_str()));

    let second_secret = format!("rotated-secret-{}", Uuid::new_v4());
    let rotated = update_feishu_settings(
        &state,
        retained.revision,
        &update(&retained, "cli_plaintext", &second_secret),
        "settings-test@example.com",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    let mut clear = update(&rotated, "", "");
    clear.clear_credentials = true;
    let cleared = update_feishu_settings(
        &state,
        rotated.revision,
        &clear,
        "settings-test@example.com",
        Uuid::new_v4(),
    )
    .await
    .unwrap();
    assert!(cleared.app_id.is_none());
    assert!(!cleared.secret_configured);
    let cleared_pair: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT app_id,app_secret FROM feishu_connector_settings WHERE connector_id=$1",
    )
    .bind(cleared.connector_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(cleared_pair, (None, None));
    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn concurrent_full_sync_triggers_create_one_active_run() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    sqlx::query(
        r#"INSERT INTO object_storage_settings
           (singleton,provider,endpoint,region,bucket,access_key_id,secret_access_key,
            key_prefix,path_style,public_base_url,updated_by)
           VALUES (true,'s3','http://127.0.0.1:19000','us-east-1','airtek-media',
                   'test-access','test-secret','media',true,
                   'http://127.0.0.1:19000/airtek-media','feishu-test')"#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"UPDATE feishu_connector_settings
           SET app_id='cli_queue',app_secret='queue-secret',
               tested_connection_revision=connection_revision"#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE source_connectors SET enabled=true WHERE connector_type='feishu'")
        .execute(&state.pool)
        .await
        .unwrap();

    let (interval, daily, manual) = tokio::join!(
        try_queue_full_sync(&state, FeishuSyncTrigger::Interval, "interval-test"),
        try_queue_full_sync(&state, FeishuSyncTrigger::Daily, "daily-test"),
        try_queue_full_sync(&state, FeishuSyncTrigger::Manual, "manual-race-test"),
    );
    let runs = [interval.unwrap(), daily.unwrap(), manual.unwrap()];
    assert_eq!(runs.iter().filter(|run| run.is_some()).count(), 1);
    let active_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM sync_runs WHERE status IN ('queued','fetching','validating','readyToPublish')",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    let counts: (i64, i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM sync_runs),
                  (SELECT count(*) FROM jobs WHERE job_type='feishuSync'),
                  (SELECT count(*) FROM feishu_run_table_results WHERE sync_run_id=$1)"#,
    )
    .bind(active_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 4));

    let response = queue_full_sync(&state, FeishuSyncTrigger::Manual, "manual-test")
        .await
        .unwrap_err()
        .into_response();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let problem: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(problem["activeRunId"], active_id.to_string());

    sqlx::query("UPDATE jobs SET status='running' WHERE id=$1")
        .bind(active_id)
        .execute(&state.pool)
        .await
        .unwrap();
    let duplicate_running = sqlx::query(
        r#"INSERT INTO jobs
           (id,job_type,status,payload,connector_id,available_at,created_at,updated_at)
           VALUES ($1,'feishuSourcePurge','running','{}'::jsonb,$2,now(),now(),now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(runs.iter().flatten().next().unwrap().connector_id)
    .execute(&state.pool)
    .await;
    assert!(duplicate_running.is_err());
    sandbox.cleanup().await;
}
