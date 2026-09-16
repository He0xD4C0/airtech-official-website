//! Current PostgreSQL readiness checks for synchronous transaction durability.

use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::{error::ApiError, flyway};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimePreparationReport {
    pub schema_current: bool,
    pub fsync: String,
    pub full_page_writes: String,
    pub synchronous_commit: String,
}

pub async fn prepare(pool: &PgPool) -> Result<RuntimePreparationReport, ApiError> {
    verify_current_postgres(pool).await
}

pub async fn verify(pool: &PgPool) -> Result<(), ApiError> {
    verify_current_postgres(pool).await.map(|_| ())
}

async fn verify_current_postgres(pool: &PgPool) -> Result<RuntimePreparationReport, ApiError> {
    let status = flyway::read_status(pool).await?;
    if !status.is_current() {
        return Err(ApiError::service_unavailable(
            "PostgreSQL is reachable but Flyway migrations are not current.",
        ));
    }
    let row = sqlx::query(
        r#"SELECT current_setting('fsync') AS fsync,
                  current_setting('full_page_writes') AS full_page_writes,
                  current_setting('synchronous_commit') AS synchronous_commit"#,
    )
    .fetch_one(pool)
    .await?;
    let fsync: String = row.try_get("fsync")?;
    let full_page_writes: String = row.try_get("full_page_writes")?;
    let synchronous_commit: String = row.try_get("synchronous_commit")?;
    if fsync != "on" || full_page_writes != "on" || synchronous_commit == "off" {
        return Err(ApiError::service_unavailable(
            "PostgreSQL must use fsync=on, full_page_writes=on and synchronous_commit other than off.",
        ));
    }
    Ok(RuntimePreparationReport {
        schema_current: true,
        fsync,
        full_page_writes,
        synchronous_commit,
    })
}
