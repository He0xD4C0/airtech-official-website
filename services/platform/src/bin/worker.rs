use airtek_platform::{worker, AppState, Config};
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
    let config = Config::from_env()?;
    if config.database_url.is_none() {
        return Err(
            "DATABASE_URL is required for the running worker; durable jobs cannot use the test-only in-memory repository."
                .into(),
        );
    }
    let state = AppState::new(config)?;
    worker::run(state).await?;
    Ok(())
}
