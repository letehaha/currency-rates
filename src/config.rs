use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    /// Default base currency for API responses when not specified by client.
    /// Note: Internal storage always uses USD as the base currency.
    pub default_api_base: String,
    /// Seed database from bundled files on startup (only if database is empty)
    pub seed_on_startup: bool,
    pub sync_on_startup: bool,
    pub sync_cron: String,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:currency_rates.db?mode=rwc".to_string()),

            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),

            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),

            default_api_base: env::var("DEFAULT_API_BASE").unwrap_or_else(|_| "USD".to_string()),

            seed_on_startup: env::var("SEED_ON_STARTUP")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),

            sync_on_startup: env::var("SYNC_ON_STARTUP")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),

            sync_cron: env::var("SYNC_CRON").unwrap_or_else(|_| "0 0 16 * * *".to_string()), // 4 PM UTC daily (after ECB publishes)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}
