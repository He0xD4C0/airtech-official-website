#![allow(dead_code)]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use uuid::Uuid;

static MIGRATION_SANDBOX_CLEANUP_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn assert_flyway_schema_current(pool: &PgPool) {
    let status = airtek_platform::flyway::read_status(pool)
        .await
        .expect("Flyway schema history must exist; run Flyway before database contract tests");

    assert!(
        status.is_current(),
        "Flyway schema status {:?} is not current; database contract tests require complete successful versions through {}",
        status,
        airtek_platform::flyway::REQUIRED_SCHEMA_VERSION,
    );
}

pub struct MigrationSandbox {
    admin_pool: PgPool,
    pool: PgPool,
    schema: String,
    connection_url: String,
}

impl MigrationSandbox {
    pub async fn create(database_url: &str) -> Self {
        let admin_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(database_url)
            .await
            .expect("PostgreSQL connection");
        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public")
            .execute(&admin_pool)
            .await
            .expect("pg_trgm extension");

        let schema = format!("migration_contract_{}", Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin_pool)
            .await
            .expect("isolated migration schema");

        let search_path = format!("{schema},public");
        let separator = if database_url.contains('?') { '&' } else { '?' };
        let connection_url =
            format!("{database_url}{separator}options=-csearch_path%3D{schema}%2Cpublic");
        let options = PgConnectOptions::from_str(database_url)
            .expect("PostgreSQL connection options")
            .options([("search_path", search_path)]);
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("isolated PostgreSQL connection");
        let pool_schema: String = sqlx::query_scalar("SELECT current_schema()")
            .fetch_one(&pool)
            .await
            .expect("isolated pool schema");
        assert_eq!(
            pool_schema, schema,
            "sandbox pool must not use public schema"
        );

        let url_probe = PgPoolOptions::new()
            .max_connections(1)
            .connect(&connection_url)
            .await
            .expect("isolated PostgreSQL URL connection");
        let url_schema: String = sqlx::query_scalar("SELECT current_schema()")
            .fetch_one(&url_probe)
            .await
            .expect("isolated URL schema");
        assert_eq!(url_schema, schema, "sandbox URL must not use public schema");
        url_probe.close().await;

        Self {
            admin_pool,
            pool,
            schema,
            connection_url,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn connection_url(&self) -> &str {
        &self.connection_url
    }

    pub async fn apply_current(&self) {
        self.apply_version_range(1, airtek_platform::flyway::REQUIRED_SCHEMA_VERSION as u32)
            .await;
    }

    pub async fn apply_version_range(&self, minimum: u32, maximum: u32) {
        for (version, path) in migration_files(minimum, maximum) {
            let sql = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!("failed to read migration {}: {error}", path.display())
            });
            let mut transaction = self.pool.begin().await.unwrap_or_else(|error| {
                panic!("failed to begin migration V{version:04} transaction: {error}")
            });
            sqlx::raw_sql(&sql)
                .execute(&mut *transaction)
                .await
                .unwrap_or_else(|error| panic!("migration V{version:04} failed: {error}"));
            transaction.commit().await.unwrap_or_else(|error| {
                panic!("failed to commit migration V{version:04}: {error}")
            });
        }
    }

    pub async fn cleanup(self) {
        self.pool.close().await;
        // Each isolated schema contains the complete platform schema. Serializing
        // DROP SCHEMA avoids exhausting PostgreSQL's shared lock table while the
        // contract tests themselves continue to run with the default parallelism.
        let _cleanup_guard = MIGRATION_SANDBOX_CLEANUP_LOCK.lock().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin_pool)
            .await
            .expect("migration schema cleanup");
        self.admin_pool.close().await;
    }
}

fn migration_files(minimum: u32, maximum: u32) -> Vec<(u32, PathBuf)> {
    assert!(minimum <= maximum, "migration range must not be empty");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut files = BTreeMap::new();

    for entry in fs::read_dir(&directory).expect("migration directory") {
        let path = entry.expect("migration directory entry").path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(version) = flyway_version(name) else {
            continue;
        };
        if version < minimum || version > maximum {
            continue;
        }
        assert!(
            files.insert(version, path).is_none(),
            "duplicate Flyway migration version V{version:04}"
        );
    }

    let actual = files.keys().copied().collect::<Vec<_>>();
    let expected = (minimum..=maximum).collect::<Vec<_>>();
    assert_eq!(
        actual, expected,
        "Flyway migration range V{minimum:04}..=V{maximum:04} is incomplete"
    );
    files.into_iter().collect()
}

fn flyway_version(name: &str) -> Option<u32> {
    let version = name.strip_prefix('V')?.split_once("__")?.0;
    name.ends_with(".sql")
        .then(|| version.parse().ok())
        .flatten()
}
