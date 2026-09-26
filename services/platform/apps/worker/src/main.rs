use airtek_jobs::worker;
use airtek_runtime::AppState;
use airtek_runtime::Config;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "airtek_runtime=info,airtek_http=info,airtek_jobs=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let config = Config::from_env()?;
    let state = AppState::new(config)?;
    state.verify_runtime_ready().await?;
    worker::run(state).await?;
    Ok(())
}
