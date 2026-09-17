#[cfg(feature = "devtools")]
use super::*;

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn object_storage_secret_is_plaintext_at_rest_but_redacted_from_reads() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let secret = format!("contract-secret-{}", Uuid::new_v4().simple());

    sqlx::query(
        r#"INSERT INTO object_storage_settings
           (singleton,provider,endpoint,region,bucket,access_key_id,secret_access_key,
            key_prefix,path_style,public_base_url,revision,updated_by)
           VALUES (true,'s3','https://s3.example.test','test-1','media','access',$1,
                   'media',true,'https://media.example.test',1,'contract@example.test')"#,
    )
    .bind(&secret)
    .execute(sandbox.pool())
    .await
    .unwrap();

    let stored: String = sqlx::query_scalar(
        "SELECT secret_access_key FROM object_storage_settings WHERE singleton=true",
    )
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(stored, secret);

    let public = airtek_platform::services::object_storage_settings::get(&state)
        .await
        .unwrap();
    let serialized = serde_json::to_string(&public).unwrap();
    assert!(public.secret_configured);
    assert!(!serialized.contains(&secret));
    assert!(!serialized.contains("secretAccessKey"));

    let runtime = airtek_platform::services::object_storage_settings::active_storage(&state)
        .await
        .unwrap();
    assert_eq!(runtime.secret_access_key, secret);

    sandbox.cleanup().await;
}
