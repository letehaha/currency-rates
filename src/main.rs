use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use std::path::PathBuf;

use currency_rates::{
    Config, EcbProvider, NbuProvider, ProviderRegistry, RatesRepository, RatesService,
    api::{self, AppState},
    seed,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "currency_rates=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::from_env();
    tracing::info!("Starting USD Currency Rates API");
    tracing::info!("Database: {}", config.database_url);
    tracing::info!("Default API base currency: {}", config.default_api_base);

    // Create database connection pool
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    // Initialize repository and schema
    let repository = RatesRepository::new(pool);
    repository.init().await?;
    tracing::info!("Database initialized");

    // Seed database from bundled files if enabled and database is empty
    if config.seed_on_startup {
        let ecb_count = repository.get_rates_count("ecb").await?;
        let nbu_count = repository.get_rates_count("nbu").await?;

        if ecb_count == 0 && nbu_count == 0 {
            tracing::info!("Database is empty, seeding from bundled files...");

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
                    let default = PathBuf::from("seed_data/nbu-full-hist.json");
                    if default.exists() {
                        Some(default)
                    } else {
                        None
                    }
                });

            if ecb_seed_path.is_some() || nbu_seed_path.is_some() {
                if let Err(e) = seed::seed_database(
                    &repository,
                    ecb_seed_path.as_deref(),
                    nbu_seed_path.as_deref(),
                )
                .await
                {
                    tracing::error!("Seeding failed: {}", e);
                } else {
                    tracing::info!("Database seeding completed");
                }
            } else {
                tracing::info!("No seed files found, skipping seeding");
            }
        } else {
            tracing::info!(
                "Database already contains data (ECB: {} records, NBU: {} records), skipping seeding",
                ecb_count,
                nbu_count
            );
        }
    }

    // Register providers
    // All providers store rates with USD as the internal base currency
    let mut providers = ProviderRegistry::new();
    providers.register(EcbProvider::new());
    providers.register(NbuProvider::new());
    tracing::info!("Registered providers: {:?}", providers.names());

    let providers = Arc::new(providers);

    // Create service
    let service = RatesService::new(
        repository.clone(),
        providers.clone(),
        config.default_api_base.clone(),
    );

    // Create shared state
    let state = Arc::new(AppState {
        service,
        default_api_base: config.default_api_base.clone(),
    });

    // Initial sync if enabled (runs in background so server starts immediately)
    if config.sync_on_startup {
        let sync_state = state.clone();
        tokio::spawn(async move {
            tracing::info!("Running initial sync in background...");
            match sync_state.service.sync_all_providers().await {
                Err(e) => {
                    tracing::error!("Initial sync failed: {}", e);
                }
                _ => {
                    tracing::info!("Initial sync completed");
                }
            }
        });
    }

    // Setup scheduled sync
    let scheduler = JobScheduler::new().await?;

    // Clone state for the scheduler
    let sync_state = state.clone();
    let cron_expr = config.sync_cron.clone();

    // Schedule periodic sync
    let job = Job::new_async(cron_expr.as_str(), move |_uuid, _lock| {
        let state = sync_state.clone();
        Box::pin(async move {
            tracing::info!("Running scheduled sync...");
            if let Err(e) = state.service.sync_all_providers().await {
                tracing::error!("Scheduled sync failed: {}", e);
            }
        })
    })?;

    scheduler.add(job).await?;
    scheduler.start().await?;
    tracing::info!("Scheduler started with cron: {}", config.sync_cron);

    // Create router
    let app = api::create_router(state);

    // Start server
    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Server listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
