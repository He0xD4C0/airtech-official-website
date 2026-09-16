use airtek_platform::services::runtime_preparation;
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

    match std::env::args().nth(1).as_deref() {
        Some("prepare-runtime") => {}
        _ => return Err("usage: airtek-maintenance prepare-runtime".into()),
    }
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required for runtime preparation")?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;
    let report = runtime_preparation::prepare(&pool).await?;
    println!("{}", serde_json::to_string(&report)?);
    pool.close().await;
    Ok(())
}
