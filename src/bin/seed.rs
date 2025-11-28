use anyhow::Result;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use currency_rates::{seed, Config, RatesRepository};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "currency_rates=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Currency Rates Database Seeder");
    tracing::info!("================================");

    // Load configuration (for database URL)
    let config = Config::from_env();
    tracing::info!("Database: {}", config.database_url);

    // Create database connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    // Initialize repository and schema
    let repository = RatesRepository::new(pool);
    repository.init().await?;
    tracing::info!("Database schema initialized");

    // Check if database already has data
    let ecb_count = repository.get_rates_count("ecb").await?;
    let nbu_count = repository.get_rates_count("nbu").await?;

    if ecb_count > 0 || nbu_count > 0 {
        tracing::warn!(
            "Database already contains data (ECB: {} records, NBU: {} records)",
            ecb_count,
            nbu_count
        );
        tracing::warn!("Seeding will add/update records. Continue? (This will take a few minutes)");
        tracing::info!("Press Ctrl+C to cancel, or wait 5 seconds to continue...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }

    // Determine seed file paths
    let ecb_seed_path = std::env::var("ECB_SEED_PATH")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let default = PathBuf::from("seed_data/ecb-full-hist.xml");
            if default.exists() {
                Some(default)
            } else {
                None
            }
        });

    let nbu_seed_path = std::env::var("NBU_SEED_PATH")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let default = PathBuf::from("seed_data/nbu-kzt-full-hist.json");
            if default.exists() {
                Some(default)
            } else {
                None
            }
        });

    if ecb_seed_path.is_none() && nbu_seed_path.is_none() {
        anyhow::bail!(
            "No seed files found. Please provide ECB_SEED_PATH and/or NBU_SEED_PATH environment variables, \
            or place seed files in seed_data/ directory:\n\
            - seed_data/ecb-full-hist.xml\n\
            - seed_data/nbu-kzt-full-hist.json"
        );
    }

    // Run seeding
    seed::seed_database(
        &repository,
        ecb_seed_path.as_deref(),
        nbu_seed_path.as_deref(),
    )
    .await?;

    tracing::info!("Seeding complete!");
    tracing::info!("You can now start the server with the pre-populated database.");

    Ok(())
}
