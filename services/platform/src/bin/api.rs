use std::net::SocketAddr;

use airtek_platform::{build_router, AppState, Config};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "airtek_platform=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    #[cfg(feature = "devtools")]
    airtek_platform::devtools::ensure_non_root()
        .map_err(|error| format!("development mode refused to start: {error:?}"))?;

    let config = Config::from_env()?;
    if config.database_url.is_none() {
        return Err(
            "DATABASE_URL is required for the running API; the in-memory repository is test-only."
                .into(),
        );
    }
    let address = SocketAddr::from(config.bind_address());
    let state = AppState::new(config)?;
    state.hydrate().await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "AIRTEKPOWER platform API listening");
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
