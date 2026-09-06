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
    /// Check the local database, Flyway history and worker backlog without exposing credentials.
    Diagnose,
    /// Idempotently load non-indexable development CMS fixtures (never products).
    Seed,
    /// Print the development OpenAPI document.
    Openapi,
    /// Inspect legacy CMS data for a lossless CMS V2 migration without writing to the database.
    Cms {
        #[command(subcommand)]
        action: CmsAction,
    },
    /// Queue Feishu synchronization work for the development worker.
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Import the confirmed Product Master into encrypted staging and queue promotion.
    Product {
        #[command(subcommand)]
        action: ProductAction,
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
enum CmsAction {
    /// Emit a read-only CMS V2 migration report and fail when blockers are present.
    Preflight,
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

#[derive(Subcommand)]
enum ProductAction {
    /// Validate and securely stage a CSV; product revisions are promoted by the worker.
    Import {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        mapping_version: Option<String>,
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

struct ProductSourceIdentity {
    data_origin: String,
    source_snapshot_id: Option<Uuid>,
    source_revision: String,
    product_import_run_id: Option<Uuid>,
    verified_import_exists: bool,
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
