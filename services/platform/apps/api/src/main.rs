use std::net::SocketAddr;

use airtek_http::build_router;
use airtek_runtime::AppState;
use airtek_runtime::Config;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "airtek_runtime=info,airtek_http=info,airtek_jobs=info,tower_http=info".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    #[cfg(feature = "devtools")]
    airtek_http::devtools::ensure_non_root()
        .map_err(|error| format!("development mode refused to start: {error:?}"))?;

    let config = Config::from_env()?;
    let address = SocketAddr::from(config.bind_address());
    let state = AppState::new(config)?;
    state.verify_runtime_ready().await?;
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
