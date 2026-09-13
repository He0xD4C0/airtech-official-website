// Minimal production-safe operations surface. The development CLI is not
// linked into production images; only diagnostics and CMS dependency checks
// are available here.

use std::{env, process::Command};

use airtek_platform::{
    flyway,
    services::{
        cms_dependency_backfill::{self, DependencyBackfillMode},
        cms_preflight,
    },
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};

const HELP: &str = "Production-safe AIRTEKPOWER operations\n\n\
Usage:\n\
  airtekctl diagnose\n\
  airtekctl cms preflight\n\
  airtekctl cms dependencies --check\n\
  airtekctl cms dependencies --apply";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperationalCommand {
    Diagnose,
    CmsPreflight,
    CmsDependencies(DependencyBackfillMode),
    Help,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let args = env::args_os()
        .skip(1)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| "refused a non-Unicode command argument")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let command = parse_operational_command(args)?;
    if command == OperationalCommand::Help {
        println!("{HELP}");
        return Ok(());
    }
    ensure_non_root()?;
    let database_url = env::var("DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or("DATABASE_URL is required for this command")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;

    match command {
        OperationalCommand::Diagnose => diagnose(&pool).await?,
        OperationalCommand::CmsPreflight => run_cms_preflight(&pool).await?,
        OperationalCommand::CmsDependencies(mode) => run_cms_dependencies(&pool, mode).await?,
        OperationalCommand::Help => unreachable!(),
    }
    Ok(())
}

fn parse_operational_command(
    args: impl IntoIterator<Item = String>,
) -> Result<OperationalCommand, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["diagnose"] => Ok(OperationalCommand::Diagnose),
        ["cms", "preflight"] => Ok(OperationalCommand::CmsPreflight),
        ["cms", "dependencies", "--check"] => Ok(OperationalCommand::CmsDependencies(
            DependencyBackfillMode::Check,
        )),
        ["cms", "dependencies", "--apply"] => Ok(OperationalCommand::CmsDependencies(
            DependencyBackfillMode::Apply,
        )),
        ["--help"] | ["-h"] => Ok(OperationalCommand::Help),
        _ => Err(format!(
            "refused unsupported operation; the production binary allows only:\n{HELP}"
        )),
    }
}

fn ensure_non_root() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("id").arg("-u").output()?;
    if !output.status.success() {
        return Err("refused to run because the operating-system UID is unavailable".into());
    }
    if String::from_utf8_lossy(&output.stdout).trim() == "0" {
        return Err("refused to run airtekctl as root".into());
    }
    Ok(())
}

async fn diagnose(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let row = sqlx::query(
        "SELECT current_database() AS database_name,current_user AS database_user,current_setting('server_version') AS server_version,to_regclass('public.jobs') IS NOT NULL AS schema_ready,to_regclass('public.flyway_schema_history') IS NOT NULL AS flyway_history_ready",
    )
    .fetch_one(pool)
    .await?;
    let schema_ready: bool = row.try_get("schema_ready")?;
    let history_ready: bool = row.try_get("flyway_history_ready")?;
    let migration = if history_ready {
        Some(flyway::read_status(pool).await?)
    } else {
        None
    };
    let (queued, failed, outbox): (i64, i64, i64) = if schema_ready {
        (
            sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status='queued'")
                .fetch_one(pool)
                .await?,
            sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status='failed'")
                .fetch_one(pool)
                .await?,
            sqlx::query_scalar(
                "SELECT count(*) FROM outbox_events WHERE status IN ('pending','processing')",
            )
            .fetch_one(pool)
            .await?,
        )
    } else {
        (0, 0, 0)
    };
    let migration_ready = migration.as_ref().is_some_and(flyway::FlywayStatus::is_current);
    print_json(json!({
        "status": if schema_ready && migration_ready { "ok" } else { "migrationRequired" },
        "database": {
            "name": row.try_get::<String, _>("database_name")?,
            "user": row.try_get::<String, _>("database_user")?,
            "serverVersion": row.try_get::<String, _>("server_version")?,
            "schemaReady": schema_ready,
        },
        "migration": {
            "tool": "flyway",
            "historyReady": history_ready,
            "currentVersion": migration.as_ref().and_then(|value| value.current_version),
            "failedMigrations": migration.as_ref().map(|value| value.failed_migrations),
            "coveredRequiredVersions": migration.as_ref().map(|value| value.covered_required_versions),
            "expectedVersion": flyway::REQUIRED_SCHEMA_VERSION,
        },
        "workerBacklog": {"queuedJobs": queued, "failedJobs": failed, "pendingOutbox": outbox},
    }))?;
    Ok(())
}

async fn run_cms_preflight(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let report = cms_preflight::run(pool).await?;
    let can_migrate = report.can_migrate;
    print_json(serde_json::to_value(report)?)?;
    if !can_migrate {
        return Err("CMS V2 migration preflight found blocking issues".into());
    }
    Ok(())
}

async fn run_cms_dependencies(
    pool: &PgPool,
    mode: DependencyBackfillMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let report = cms_dependency_backfill::run(pool, mode).await?;
    let can_release = report.can_release();
    print_json(serde_json::to_value(report)?)?;
    if !can_release {
        return Err("CMS publication dependency backfill found blocking issues".into());
    }
    Ok(())
}

fn print_json(value: Value) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<OperationalCommand, String> {
        parse_operational_command(args.iter().map(|value| (*value).to_owned()))
    }

    #[test]
    fn accepts_only_reviewed_operational_commands() {
        assert_eq!(parse(&["diagnose"]).unwrap(), OperationalCommand::Diagnose);
        assert_eq!(
            parse(&["cms", "dependencies", "--apply"]).unwrap(),
            OperationalCommand::CmsDependencies(DependencyBackfillMode::Apply)
        );
    }

    #[test]
    fn refuses_development_and_removed_media_operations() {
        for args in [
            &["seed"][..],
            &["openapi"][..],
            &["jobs", "retry"][..],
            &["media", "preflight"][..],
            &["cms", "dependencies"][..],
        ] {
            assert!(parse(args)
                .unwrap_err()
                .contains("refused unsupported operation"));
        }
    }
}
