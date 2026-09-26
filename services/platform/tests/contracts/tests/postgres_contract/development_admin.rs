#[cfg(feature = "devtools")]
use super::*;

#[cfg(feature = "devtools")]
use airtek_runtime::services::development_admin::{self, DevelopmentAdminInput};

#[cfg(feature = "devtools")]
fn input() -> DevelopmentAdminInput {
    DevelopmentAdminInput {
        display_name: "AIRTEK Local Administrator".into(),
        email: "local-admin@airtek.invalid".into(),
        password: "Airtek-Local-Admin-20260917!".into(),
    }
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn development_admin_seed_is_idempotent_and_skips_non_empty_databases() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;

    let first = development_admin::ensure(sandbox.pool(), &input())
        .await
        .unwrap();
    assert_eq!(first.status, "created");
    let password_hash: String = sqlx::query_scalar(
        "SELECT password_hash FROM users WHERE email='local-admin@airtek.invalid'",
    )
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert!(password_hash.starts_with("$argon2id$"));
    assert!(!password_hash.contains("Airtek-Local-Admin-20260917!"));
    let role: String = sqlx::query_scalar(
        r#"SELECT roles.key FROM roles JOIN user_roles ON user_roles.role_id=roles.id
           JOIN users ON users.id=user_roles.user_id
           WHERE users.email='local-admin@airtek.invalid' AND roles.key='super-admin'"#,
    )
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(role, "super-admin");

    let repeated = development_admin::ensure(sandbox.pool(), &input())
        .await
        .unwrap();
    assert_eq!(repeated.status, "skippedExistingUsers");
    let retained_hash: String = sqlx::query_scalar(
        "SELECT password_hash FROM users WHERE email='local-admin@airtek.invalid'",
    )
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(retained_hash, password_hash);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn explicit_development_admin_reset_targets_only_the_reserved_account() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let other_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO users (id,email,password_hash,display_name,status,created_at,updated_at)
           VALUES ($1,'existing@airtek.invalid','unchanged-hash','Existing Admin','active',now(),now())"#,
    )
    .bind(other_id)
    .execute(sandbox.pool())
    .await
    .unwrap();

    let result = development_admin::reset(sandbox.pool(), &input())
        .await
        .unwrap();
    assert_eq!(result.status, "reset");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(count, 2);
    let other_hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
        .bind(other_id)
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(other_hash, "unchanged-hash");
    let reset_state: (String, bool, bool) = sqlx::query_as(
        r#"SELECT status,totp_secret_ciphertext IS NULL,totp_confirmed_at IS NULL
           FROM users WHERE email='local-admin@airtek.invalid'"#,
    )
    .fetch_one(sandbox.pool())
    .await
    .unwrap();
    assert_eq!(reset_state, ("active".into(), true, true));
    sandbox.cleanup().await;
}

#[tokio::test]
#[cfg(feature = "devtools")]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn concurrent_development_seed_creates_only_one_user() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let first_pool = sandbox.pool().clone();
    let second_pool = sandbox.pool().clone();
    let (first, second) = tokio::join!(
        async move {
            development_admin::ensure(&first_pool, &input())
                .await
                .unwrap()
        },
        async move {
            development_admin::ensure(&second_pool, &input())
                .await
                .unwrap()
        },
    );
    let statuses = [first.status, second.status];
    assert!(statuses.contains(&"created"));
    assert!(statuses.contains(&"skippedExistingUsers"));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(sandbox.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    sandbox.cleanup().await;
}
