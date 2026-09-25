#[cfg(feature = "devtools")]
use airtek_platform::services::development_admin::{self, DevelopmentAdminInput};
#[cfg(feature = "devtools")]
use airtek_platform::services::development_public_site;
use airtek_platform::services::{public_readiness, runtime_preparation};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "airtek_platform=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let command = std::env::args().nth(1);
    airtek_platform::config::reject_development_seed_configuration()?;
    validate_command(command.as_deref())?;
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required for runtime preparation")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;
    let output = match command.as_deref() {
        Some("inspect-public-site") => {
            airtek_platform::services::public_site_inventory::inspect(&pool).await?
        }
        Some("prepare-runtime") => {
            serde_json::to_value(runtime_preparation::prepare(&pool).await?)?
        }
        Some("check-public-readiness") => {
            serde_json::to_value(public_readiness::check(&pool).await?)?
        }
        #[cfg(feature = "devtools")]
        Some("prepare-development-runtime") => {
            let runtime = runtime_preparation::prepare(&pool).await?;
            let development_admin = if development_seed_enabled()? {
                Some(development_admin::ensure(&pool, &development_admin_input()?).await?)
            } else {
                None
            };
            let development_public_site = if development_public_seed_enabled()? {
                Some(
                    development_public_site::ensure(
                        &pool,
                        &required_env("AIRTEK_DEV_ADMIN_EMAIL")?,
                    )
                    .await?,
                )
            } else {
                None
            };
            serde_json::json!({
                "runtime": runtime,
                "developmentAdmin": development_admin,
                "developmentPublicSite": development_public_site,
            })
        }
        #[cfg(feature = "devtools")]
        Some("reset-development-admin") => {
            require_reset_confirmation()?;
            serde_json::to_value(
                development_admin::reset(&pool, &development_admin_input()?).await?,
            )?
        }
        _ => unreachable!("command was validated before connecting"),
    };
    println!("{}", serde_json::to_string(&output)?);
    pool.close().await;
    Ok(())
}

fn validate_command(command: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Some("prepare-runtime" | "check-public-readiness" | "inspect-public-site") => Ok(()),
        #[cfg(feature = "devtools")]
        Some("prepare-development-runtime" | "reset-development-admin") => Ok(()),
        _ => Err(usage().into()),
    }
}

fn usage() -> &'static str {
    #[cfg(feature = "devtools")]
    {
        "usage: airtek-maintenance prepare-runtime|inspect-public-site|check-public-readiness|prepare-development-runtime|reset-development-admin"
    }
    #[cfg(not(feature = "devtools"))]
    {
        "usage: airtek-maintenance prepare-runtime|inspect-public-site|check-public-readiness"
    }
}

#[cfg(feature = "devtools")]
fn development_seed_enabled() -> Result<bool, Box<dyn std::error::Error>> {
    match std::env::var("AIRTEK_DEV_ADMIN_SEED")
        .unwrap_or_else(|_| "false".into())
        .as_str()
    {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("AIRTEK_DEV_ADMIN_SEED must be true or false".into()),
    }
}

#[cfg(feature = "devtools")]
fn development_public_seed_enabled() -> Result<bool, Box<dyn std::error::Error>> {
    match std::env::var("AIRTEK_DEV_PUBLIC_SEED")
        .unwrap_or_else(|_| "false".into())
        .as_str()
    {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err("AIRTEK_DEV_PUBLIC_SEED must be true or false".into()),
    }
}

#[cfg(feature = "devtools")]
fn development_admin_input() -> Result<DevelopmentAdminInput, Box<dyn std::error::Error>> {
    Ok(DevelopmentAdminInput {
        display_name: required_env("AIRTEK_DEV_ADMIN_DISPLAY_NAME")?,
        email: required_env("AIRTEK_DEV_ADMIN_EMAIL")?,
        password: required_env("AIRTEK_DEV_ADMIN_PASSWORD")?,
    })
}

#[cfg(feature = "devtools")]
fn require_reset_confirmation() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("AIRTEK_ALLOW_DEV_ADMIN_RESET").as_deref() == Ok("true") {
        Ok(())
    } else {
        Err("AIRTEK_ALLOW_DEV_ADMIN_RESET=true is required for an explicit reset".into())
    }
}

#[cfg(feature = "devtools")]
fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required").into())
}
