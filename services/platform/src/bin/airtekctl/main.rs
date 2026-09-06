#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    ensure_development_runtime()?;
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
        Command::Seed => {
            if config.production {
                return Err("refused to seed: production runtime is not allowed".into());
            }
            let report = development_seed::seed(&pool, &cli_actor()).await?;
            print_json(serde_json::to_value(report)?)?;
        }
        Command::Cms {
            action: CmsAction::Preflight,
        } => {
            let report = cms_preflight::run(&pool).await?;
            let can_migrate = report.can_migrate;
            print_json(serde_json::to_value(report)?)?;
            if !can_migrate {
                return Err("CMS V2 migration preflight found blocking issues".into());
            }
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
        Command::Product {
            action:
                ProductAction::Import {
                    file,
                    mapping_version,
                },
        } => {
            let mapping_version =
                mapping_version.unwrap_or_else(|| config.product_import_mapping_version.clone());
            validate_mapping_version(&mapping_version)?;
            let metadata = std::fs::metadata(&file)
                .map_err(|error| format!("unable to inspect Product Master CSV: {error}"))?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 16 * 1024 * 1024 {
                return Err(
                    "Product Master CSV must be a regular file between 1 byte and 16 MiB".into(),
                );
            }
            let csv = std::fs::read_to_string(&file)
                .map_err(|error| format!("unable to read Product Master CSV: {error}"))?;
            let parsed = parse_product_master(
                &csv,
                &mapping_version,
                config.product_staging_encryption_key.as_ref(),
            )?;
            let environment = if config.production {
                "production"
            } else {
                "development"
            };
            let staged = stage_and_queue_product_import(
                &pool,
                parsed,
                environment,
                config.approved_product_master.as_ref(),
                None,
                &cli_actor(),
            )
            .await?;
            let warning_count = staged
                .result
                .errors
                .iter()
                .filter(|error| error.severity == "warning")
                .count();
            let status = if staged.queued {
                "queued"
            } else {
                staged.result.status.as_str()
            };
            print_json(json!({
                "operationId": staged.operation_id,
                "importRunId": staged.result.id,
                "status": status,
                "reused": staged.result.reused,
                "checksum": staged.result.checksum,
                "mappingVersion": staged.result.mapping_version,
                "totalRows": staged.result.total_rows,
                "validRows": staged.result.valid_rows,
                "malformedRows": staged.result.malformed_rows,
                "warningCount": warning_count,
                "missingAssetCount": staged.result.missing_assets.len()
            }))?;
        }
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
