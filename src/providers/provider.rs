use async_trait::async_trait;
use chrono::NaiveDate;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::Result;
use crate::models::{Currency, DailyRates};

/// Trait that all currency rate providers must implement.
/// This allows easy addition of new data sources.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Unique identifier for this provider
    fn name(&self) -> &str;

    /// Human-readable description
    fn description(&self) -> &str;

    /// List of currencies this provider supports
    async fn supported_currencies(&self) -> Result<Vec<Currency>>;

    /// Fetch the latest available rates
    async fn fetch_latest(&self) -> Result<DailyRates>;

    /// Fetch rates for a specific date
    async fn fetch_date(&self, date: NaiveDate) -> Result<DailyRates>;

    /// Fetch rates for a date range (batch operation)
    /// Default implementation calls fetch_date for each day
    async fn fetch_range(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<DailyRates>> {
        let mut results = Vec::new();
        let mut current = start;

        while current <= end {
            match self.fetch_date(current).await {
                Ok(rates) => results.push(rates),
                Err(e) => {
                    tracing::warn!(
                        "Failed to fetch rates for {} from {}: {}",
                        current,
                        self.name(),
                        e
                    );
                }
            }
            current = current.succ_opt().unwrap_or(current);
        }

        Ok(results)
    }

    /// Fetch full historical data (if provider supports it)
    async fn fetch_full_history(&self) -> Result<Vec<DailyRates>>;
}

/// Registry of all available providers
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a new provider
    pub fn register<P: Provider + 'static>(&mut self, provider: P) {
        let name = provider.name().to_string();
        self.providers.insert(name, Arc::new(provider));
    }

    /// Get a provider by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(name).cloned()
    }

    /// Get all registered providers
    pub fn all(&self) -> Vec<Arc<dyn Provider>> {
        self.providers.values().cloned().collect()
    }

    /// Get provider names
    pub fn names(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
