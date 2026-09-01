use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required by airtek-migrate")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    println!("AIRTEKPOWER platform migrations are current");
    Ok(())
}
