use airtek_platform::{
    devtools,
    models::{ContentEntry, FactState, Product, SyncRun, SyncRunStatus},
    openapi,
    services::feishu::validate_staging_payload,
    Config,
};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Parser)]
#[command(
    name = "airtekctl",
    about = "Development-only AIRTEKPOWER platform CLI",
    long_about = "Development-only, non-root operations for the AIRTEKPOWER local platform. Queued provider work is never reported as completed by this CLI."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check the local database, migrations and worker backlog without exposing credentials.
    Diagnose,
    /// Apply the embedded SQLx migration set to the development database.
    Migrate,
    /// Print the development OpenAPI document.
    Openapi,
    /// Queue Feishu synchronization work for the development worker.
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Validate stored CMS, catalog and source-staging records.
    Validate {
        #[arg(value_enum, default_value_t = ValidationTarget::All)]
        target: ValidationTarget,
    },
    /// Manage the development search projection.
    Index {
        #[command(subcommand)]
        action: IndexAction,
    },
    /// Manage development cache invalidation work.
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Inspect or retry bounded background jobs.
    Jobs {
        #[command(subcommand)]
        action: JobsAction,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// Queue validation/diff work that must not publish product data.
    DryRun {
        #[arg(long, default_value = "development")]
        mapping_version: String,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Queue a normal staging sync; publishing remains a separate Admin action.
    Queue {
        #[arg(long, default_value = "development")]
        mapping_version: String,
        #[arg(long)]
        cursor: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ValidationTarget {
    All,
    Content,
    Products,
    Staging,
}

#[derive(Subcommand)]
enum IndexAction {
    /// Queue a search reindex. The worker fails honestly when no adapter is configured.
    Rebuild,
}

#[derive(Subcommand)]
enum CacheAction {
    /// Queue cache invalidation. The worker fails honestly when no adapter is configured.
    Invalidate,
}

#[derive(Subcommand)]
enum JobsAction {
    /// List recent jobs without printing their possibly sensitive payloads.
    List {
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=200))]
        limit: u16,
    },
    /// Retry one failed job with a fresh bounded attempt budget.
    Retry { id: Uuid },
    /// Queue one of the platform's predefined internal maintenance jobs.
    Queue {
        #[arg(value_enum)]
        job_type: MaintenanceJob,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum MaintenanceJob {
    MigrationPreflight,
    RetentionApply,
}

impl MaintenanceJob {
    fn as_job_type(self) -> &'static str {
        match self {
            Self::MigrationPreflight => "migrationPreflight",
            Self::RetentionApply => "retentionApply",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationFinding {
    entity_type: &'static str,
    entity_id: String,
    field_path: String,
    code: String,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationReport {
    records_checked: u64,
    findings: Vec<ValidationFinding>,
    truncated: bool,
}

impl ValidationReport {
    fn push(&mut self, finding: ValidationFinding) {
        if self.findings.len() < 100 {
            self.findings.push(finding);
        } else {
            self.truncated = true;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    devtools::ensure_non_root().map_err(|error| format!("refused to run: {error}"))?;
    let cli = Cli::parse();
    if matches!(cli.command, Command::Openapi) {
        println!("{}", serde_json::to_string_pretty(&openapi::document())?);
        return Ok(());
    }

    let config = Config::from_env()?;
    let database_url = config
        .database_url
        .ok_or("DATABASE_URL is required for this command")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;

    match cli.command {
        Command::Diagnose => diagnose(&pool).await?,
        Command::Migrate => {
            sqlx::migrate!("./migrations").run(&pool).await?;
            record_cli_audit(
                &pool,
                "devtools.cli.migrate.completed",
                "database",
                None,
                json!({"migrationSet": "embedded-sqlx"}),
                "Development database migration command completed.",
            )
            .await?;
            print_json(json!({"status": "completed", "operation": "migrate"}))?;
        }
        Command::Sync { action } => match action {
            SyncAction::DryRun {
                mapping_version,
                cursor,
            } => {
                validate_mapping_version(&mapping_version)?;
                validate_cursor(&cursor)?;
                queue_feishu_sync(&pool, true, mapping_version, cursor).await?;
            }
            SyncAction::Queue {
                mapping_version,
                cursor,
            } => {
                validate_mapping_version(&mapping_version)?;
                validate_cursor(&cursor)?;
                queue_feishu_sync(&pool, false, mapping_version, cursor).await?;
            }
        },
        Command::Validate { target } => validate(&pool, target).await?,
        Command::Index {
            action: IndexAction::Rebuild,
        } => {
            queue_operation(
                &pool,
                "searchReindex",
                "Queue development search projection rebuild from airtekctl",
            )
            .await?;
        }
        Command::Cache {
            action: CacheAction::Invalidate,
        } => {
            queue_operation(
                &pool,
                "cacheInvalidate",
                "Queue development cache invalidation from airtekctl",
            )
            .await?;
        }
        Command::Jobs { action } => match action {
            JobsAction::List { limit } => list_jobs(&pool, limit).await?,
            JobsAction::Retry { id } => retry_job(&pool, id).await?,
            JobsAction::Queue { job_type } => {
                queue_operation(
                    &pool,
                    job_type.as_job_type(),
                    "Queue predefined development maintenance job from airtekctl",
                )
                .await?;
            }
        },
        Command::Openapi => unreachable!(),
    }
    Ok(())
}

async fn diagnose(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let row = sqlx::query(
        "SELECT current_database() AS database_name, current_user AS database_user, current_setting('server_version') AS server_version, to_regclass('public.jobs') IS NOT NULL AS schema_ready",
    )
    .fetch_one(pool)
    .await?;
    let schema_ready: bool = row.try_get("schema_ready")?;
    let (queued_jobs, failed_jobs, pending_outbox): (i64, i64, i64) = if schema_ready {
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
    print_json(json!({
        "status": if schema_ready { "ok" } else { "migrationRequired" },
        "database": {
            "name": row.try_get::<String, _>("database_name")?,
            "user": row.try_get::<String, _>("database_user")?,
            "serverVersion": row.try_get::<String, _>("server_version")?,
            "schemaReady": schema_ready,
        },
        "workerBacklog": {
            "queuedJobs": queued_jobs,
            "failedJobs": failed_jobs,
            "pendingOutbox": pending_outbox,
        },
        "osUid": std::process::Command::new("id").arg("-u").output().ok().map(|value| String::from_utf8_lossy(&value.stdout).trim().to_owned()),
    }))?;
    Ok(())
}

async fn queue_feishu_sync(
    pool: &PgPool,
    dry_run: bool,
    mapping_version: String,
    cursor: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = Uuid::new_v4();
    let started_at = Utc::now();
    let run = SyncRun {
        id,
        source: "feishu".into(),
        dry_run,
        mapping_version,
        status: SyncRunStatus::Queued,
        resume_cursor: cursor,
        records_seen: 0,
        records_valid: 0,
        conflict_count: 0,
        started_at,
        completed_at: None,
        error: None,
    };
    let run_payload = serde_json::to_value(&run)?;
    let job_payload = json!({
        "source": "airtekctl",
        "syncRunId": id,
        "dryRun": dry_run,
        "mappingVersion": &run.mapping_version,
        "cursor": &run.resume_cursor,
    });
    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"INSERT INTO sync_runs
           (id, source, dry_run, mapping_version, status, resume_cursor, records_seen,
            records_valid, conflict_count, started_at, completed_at, payload)
           VALUES ($1,'feishu',$2,$3,'queued',$4,0,0,0,$5,NULL,$6)"#,
    )
    .bind(id)
    .bind(dry_run)
    .bind(&run.mapping_version)
    .bind(&run.resume_cursor)
    .bind(started_at)
    .bind(run_payload)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO jobs (id, job_type, status, payload, available_at, created_at, updated_at) VALUES ($1,'feishuSync','queued',$2,$3,$3,$3)",
    )
    .bind(id)
    .bind(job_payload)
    .bind(started_at)
    .execute(&mut *transaction)
    .await?;
    insert_cli_audit(
        &mut transaction,
        "devtools.cli.feishuSync.queued",
        "syncRun",
        Some(id),
        json!({"dryRun": dry_run, "mappingVersion": &run.mapping_version}),
        "Development Feishu staging synchronization queued; no product was published.",
    )
    .await?;
    transaction.commit().await?;
    print_json(json!({
        "status": "queued",
        "jobId": id,
        "syncRunId": id,
        "jobType": "feishuSync",
        "dryRun": dry_run,
        "note": "Queued in Feishu staging; this does not publish products. The worker will fail explicitly when no Feishu provider adapter is configured.",
    }))?;
    Ok(())
}

async fn queue_operation(
    pool: &PgPool,
    job_type: &'static str,
    reason: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO operation_runs (id, kind, status, reason, created_at, updated_at) VALUES ($1,$2,'queued',$3,$4,$4)",
    )
    .bind(id)
    .bind(job_type)
    .bind(reason)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO jobs (id, job_type, status, payload, available_at, created_at, updated_at) VALUES ($1,$2,'queued',$3,$4,$4,$4)",
    )
    .bind(id)
    .bind(job_type)
    .bind(json!({"source": "airtekctl"}))
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    insert_cli_audit(
        &mut transaction,
        "devtools.cli.operation.queued",
        "backgroundOperation",
        Some(id),
        json!({"jobType": job_type}),
        reason,
    )
    .await?;
    transaction.commit().await?;
    print_json(json!({
        "status": "queued",
        "jobId": id,
        "operationId": id,
        "jobType": job_type,
        "note": "Queued is not completed; the worker may fail the job when its required provider adapter is not configured.",
    }))?;
    Ok(())
}

async fn list_jobs(pool: &PgPool, limit: u16) -> Result<(), Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT id, job_type, status, attempts, max_attempts, available_at, last_error, created_at, updated_at FROM jobs ORDER BY created_at DESC LIMIT $1",
    )
    .bind(i64::from(limit))
    .fetch_all(pool)
    .await?;
    let jobs: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            Ok(json!({
                "id": row.try_get::<Uuid, _>("id")?,
                "jobType": row.try_get::<String, _>("job_type")?,
                "status": row.try_get::<String, _>("status")?,
                "attempts": row.try_get::<i32, _>("attempts")?,
                "maxAttempts": row.try_get::<i32, _>("max_attempts")?,
                "availableAt": row.try_get::<chrono::DateTime<chrono::Utc>, _>("available_at")?,
                "lastError": row.try_get::<Option<String>, _>("last_error")?,
                "createdAt": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")?,
                "updatedAt": row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at")?,
            }))
        })
        .collect::<Result<_, sqlx::Error>>()?;
    print_json(json!({"items": jobs}))?;
    Ok(())
}

async fn retry_job(pool: &PgPool, id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
    let mut transaction = pool.begin().await?;
    let job_type = sqlx::query_scalar::<_, String>(
        "UPDATE jobs SET status='queued', attempts=0, available_at=now(), last_error=NULL, updated_at=now() WHERE id=$1 AND status='failed' RETURNING job_type",
    )
    .bind(id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or("Only a failed job can be retried")?;
    sqlx::query("UPDATE operation_runs SET status='queued', updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if job_type == "feishuSync" {
        sqlx::query(
            "UPDATE sync_runs SET status='queued', completed_at=NULL, payload=(jsonb_set(payload, '{status}', '\"queued\"', true) - 'error' - 'completedAt') WHERE id=$1",
        )
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }
    insert_cli_audit(
        &mut transaction,
        "devtools.cli.job.retried",
        "backgroundJob",
        Some(id),
        json!({"jobType": &job_type, "attemptsReset": true}),
        "Failed development job queued with a fresh bounded attempt budget.",
    )
    .await?;
    transaction.commit().await?;
    print_json(json!({
        "status": "queued",
        "jobId": id,
        "jobType": job_type,
        "note": "The retry was queued; it has not completed.",
    }))?;
    Ok(())
}

async fn validate(
    pool: &PgPool,
    target: ValidationTarget,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut report = ValidationReport::default();
    if matches!(target, ValidationTarget::All | ValidationTarget::Content) {
        validate_content(pool, &mut report).await?;
    }
    if matches!(target, ValidationTarget::All | ValidationTarget::Products) {
        validate_products(pool, &mut report).await?;
    }
    if matches!(target, ValidationTarget::All | ValidationTarget::Staging) {
        validate_staging(pool, &mut report).await?;
    }
    let passed = report.findings.is_empty();
    print_json(json!({
        "status": if passed { "valid" } else { "invalid" },
        "report": report,
    }))?;
    if !passed {
        return Err("validation failed; see the structured findings above".into());
    }
    Ok(())
}

async fn validate_content(pool: &PgPool, report: &mut ValidationReport) -> Result<(), sqlx::Error> {
    for row in sqlx::query("SELECT id, payload FROM content_entries ORDER BY id")
        .fetch_all(pool)
        .await?
    {
        let id: Uuid = row.try_get("id")?;
        let payload: Value = row.try_get("payload")?;
        report.records_checked += 1;
        match serde_json::from_value::<ContentEntry>(payload) {
            Ok(content) => {
                if content.body.schema_version != 1 {
                    report.push(finding(
                        "content",
                        id,
                        "body.schemaVersion",
                        "unsupportedSchema",
                    ));
                }
                if content.body.doc.get("type").and_then(Value::as_str) != Some("doc") {
                    report.push(finding("content", id, "body.doc.type", "docRequired"));
                }
                if content.slug.trim().is_empty() || content.locale.trim().is_empty() {
                    report.push(finding("content", id, "slug", "routeIdentityRequired"));
                }
            }
            Err(_) => report.push(finding("content", id, "$", "invalidStoredPayload")),
        }
    }
    Ok(())
}

async fn validate_products(
    pool: &PgPool,
    report: &mut ValidationReport,
) -> Result<(), sqlx::Error> {
    for row in sqlx::query("SELECT id, payload FROM products ORDER BY id")
        .fetch_all(pool)
        .await?
    {
        let id: Uuid = row.try_get("id")?;
        let payload: Value = row.try_get("payload")?;
        report.records_checked += 1;
        match serde_json::from_value::<Product>(payload) {
            Ok(product) => validate_product(id, &product, report),
            Err(_) => report.push(finding("product", id, "$", "invalidStoredPayload")),
        }
    }
    Ok(())
}

fn validate_product(id: Uuid, product: &Product, report: &mut ValidationReport) {
    if product.stable_id.trim().is_empty()
        || product.source_snapshot_id.is_nil()
        || product.source_revision.trim().is_empty()
    {
        report.push(finding(
            "product",
            id,
            "sourceRevision",
            "sourceIdentityRequired",
        ));
    }
    for (index, specification) in product.specifications.iter().enumerate() {
        if specification.state == FactState::Verified
            && (specification.value.is_none()
                || specification
                    .source_reference
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty()))
        {
            report.push(finding(
                "product",
                id,
                &format!("specifications[{index}]"),
                "verifiedFactRequiresValueAndSource",
            ));
        }
    }
    for (curve_index, curve) in product.performance_curves.iter().enumerate() {
        if curve.points.len() < 2
            || curve.source_reference.trim().is_empty()
            || curve.points.iter().any(|point| {
                !point.airflow.is_finite()
                    || !point.pressure.is_finite()
                    || point.airflow < 0.0
                    || point.pressure < 0.0
            })
            || curve
                .points
                .windows(2)
                .any(|points| points[0].airflow >= points[1].airflow)
        {
            report.push(finding(
                "product",
                id,
                &format!("performanceCurves[{curve_index}]"),
                "invalidPerformanceCurve",
            ));
        }
        if curve.state == FactState::Verified
            && curve
                .test_method
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            report.push(finding(
                "product",
                id,
                &format!("performanceCurves[{curve_index}].testMethod"),
                "verifiedCurveRequiresTestMethod",
            ));
        }
    }
}

async fn validate_staging(pool: &PgPool, report: &mut ValidationReport) -> Result<(), sqlx::Error> {
    for row in sqlx::query(
        "SELECT source_record_id, source_payload FROM source_snapshots ORDER BY received_at DESC",
    )
    .fetch_all(pool)
    .await?
    {
        let source_record_id: String = row.try_get("source_record_id")?;
        let payload: Value = row.try_get("source_payload")?;
        report.records_checked += 1;
        for issue in validate_staging_payload(&payload) {
            report.push(ValidationFinding {
                entity_type: "sourceSnapshot",
                entity_id: source_record_id.clone(),
                field_path: issue.field_path,
                code: issue.code,
            });
        }
    }
    Ok(())
}

fn finding(
    entity_type: &'static str,
    entity_id: Uuid,
    field_path: &str,
    code: &str,
) -> ValidationFinding {
    ValidationFinding {
        entity_type,
        entity_id: entity_id.to_string(),
        field_path: field_path.into(),
        code: code.into(),
    }
}

fn validate_mapping_version(value: &str) -> Result<(), Box<dyn std::error::Error>> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 {
        return Err("mapping version must contain 1 to 120 characters".into());
    }
    Ok(())
}

fn validate_cursor(value: &Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    if value.as_ref().is_some_and(|value| {
        value.trim().is_empty() || value.len() > 2_048 || value.chars().any(char::is_control)
    }) {
        return Err("cursor must be a non-empty opaque value of at most 2048 characters".into());
    }
    Ok(())
}

async fn insert_cli_audit(
    transaction: &mut Transaction<'_, Postgres>,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    after: Value,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id, actor, action, entity_type, entity_id, before_value, after_value,
            reason, request_id, occurred_at)
           VALUES ($1,$2,$3,$4,$5,NULL,$6,$7,$8,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(cli_actor())
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(after)
    .bind(reason)
    .bind(Uuid::new_v4())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn record_cli_audit(
    pool: &PgPool,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    after: Value,
    reason: &str,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    insert_cli_audit(
        &mut transaction,
        action,
        entity_type,
        entity_id,
        after,
        reason,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

fn cli_actor() -> String {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|value| value.status.success())
        .map(|value| format!("osUid:{}", String::from_utf8_lossy(&value.stdout).trim()))
        .unwrap_or_else(|| "osUid:unavailable".into())
}

fn print_json(value: Value) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
