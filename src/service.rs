use chrono::NaiveDate;
use std::collections::HashMap;
use std::sync::Arc;

use crate::db::RatesRepository;
use crate::error::{AppError, Result};
use crate::models::{RatesResponse, TimeSeriesResponse};
use crate::providers::ProviderRegistry;

/// Internal storage base currency - all providers store rates relative to USD
const INTERNAL_BASE: &str = "USD";

/// Service for currency rate operations
/// Handles base currency conversion and data aggregation
///
/// Note: All rates are stored internally with USD as the base currency.
/// Conversion to other bases happens at query time.
pub struct RatesService {
    repository: RatesRepository,
    providers: Arc<ProviderRegistry>,
    /// Default base currency for API responses (when client doesn't specify)
    default_api_base: String,
}

impl RatesService {
    pub fn new(
        repository: RatesRepository,
        providers: Arc<ProviderRegistry>,
        default_api_base: String,
    ) -> Self {
        Self {
            repository,
            providers,
            default_api_base,
        }
    }

    /// Convert rates from one base currency to another
    /// If rates are EUR-based and we want USD-based:
    /// New rate = Original EUR rate / EUR->USD rate
    fn convert_base_currency(
        rates: &HashMap<String, f64>,
        from_base: &str,
        to_base: &str,
    ) -> Result<HashMap<String, f64>> {
        if from_base == to_base {
            return Ok(rates.clone());
        }

        // Get the conversion rate from the target base in the original rates
        // e.g., if from_base=EUR, to_base=USD, we need the USD rate in EUR terms
        let conversion_rate = rates
            .get(to_base)
            .ok_or_else(|| AppError::InvalidCurrency(to_base.to_string()))?;

        let mut converted: HashMap<String, f64> = HashMap::new();

        for (currency, rate) in rates {
            if currency == to_base {
                // The new base currency has rate 1.0
                continue;
            }

            // Convert: if 1 EUR = 1.05 USD and 1 EUR = 160 JPY
            // Then 1 USD = 160/1.05 JPY
            let new_rate = rate / conversion_rate;
            converted.insert(currency.clone(), new_rate);
        }

        // Add the original base as a currency
        converted.insert(from_base.to_string(), 1.0 / conversion_rate);

        Ok(converted)
    }

    /// Round rate to reasonable precision (6 decimal places)
    fn round_rate(rate: f64) -> f64 {
        (rate * 1_000_000.0).round() / 1_000_000.0
    }

    /// Apply rounding to all rates (for future use)
    #[allow(dead_code)]
    fn round_rates(rates: HashMap<String, f64>) -> HashMap<String, f64> {
        rates
            .into_iter()
            .map(|(k, v)| (k, Self::round_rate(v)))
            .collect()
    }

    /// Sync rates from all providers
    pub async fn sync_all_providers(&self) -> Result<()> {
        for provider in self.providers.all() {
            tracing::info!("Syncing rates from provider: {}", provider.name());

            match self.sync_provider(provider.name()).await {
                Ok(count) => {
                    tracing::info!("Synced {} rates from {}", count, provider.name());
                    self.repository
                        .log_sync(provider.name(), count, "success")
                        .await?;
                }
                Err(e) => {
                    tracing::error!("Failed to sync {}: {}", provider.name(), e);
                    self.repository
                        .log_sync(provider.name(), 0, &format!("error: {}", e))
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Sync rates from a specific provider
    pub async fn sync_provider(&self, provider_name: &str) -> Result<usize> {
        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| AppError::Provider(format!("Unknown provider: {}", provider_name)))?;

        // Get last sync date
        let last_date = self.repository.get_latest_date(Some(provider_name)).await?;

        let rates = if let Some(last) = last_date {
            // Fetch only new data
            let today = chrono::Utc::now().date_naive();
            if last >= today {
                tracing::info!("Provider {} is already up to date", provider_name);
                return Ok(0);
            }
            provider.fetch_range(last, today).await?
        } else {
            // First sync - fetch full history
            tracing::info!("First sync for {}, fetching full history", provider_name);
            provider.fetch_full_history().await?
        };

        // Store rates
        let count = self.repository.store_daily_rates_batch(&rates).await?;

        // Store currencies
        let currencies = provider.supported_currencies().await?;
        let currency_pairs: Vec<(String, String)> =
            currencies.into_iter().map(|c| (c.code, c.name)).collect();
        self.repository
            .store_currencies(&currency_pairs, provider_name)
            .await?;

        Ok(count)
    }

    /// Get latest rates
    pub async fn get_latest(
        &self,
        base: Option<&str>,
        symbols: Option<&[String]>,
        amount: Option<f64>,
    ) -> Result<RatesResponse> {
        let base = base.unwrap_or(&self.default_api_base);
        let amount = amount.unwrap_or(1.0);

        // Get the latest date
        let date = self
            .repository
            .get_latest_date(None)
            .await?
            .ok_or(AppError::NoDataAvailable)?;

        self.get_rates_for_date(date, base, symbols, amount).await
    }

    /// Get rates for a specific date
    pub async fn get_rates_for_date(
        &self,
        date: NaiveDate,
        base: &str,
        symbols: Option<&[String]>,
        amount: f64,
    ) -> Result<RatesResponse> {
        tracing::debug!("get_rates_for_date: date={}, base={}", date, base);

        // All rates are stored internally as USD-based
        let usd_rates = self
            .repository
            .get_rates_for_date(date, INTERNAL_BASE, None)
            .await?;

        tracing::debug!("{}-based rates found: {}", INTERNAL_BASE, usd_rates.len());

        if usd_rates.is_empty() {
            return Err(AppError::NoDataAvailable);
        }

        // Add USD = 1.0 to the rates for conversion
        let mut full_rates = usd_rates.clone();
        full_rates.insert(INTERNAL_BASE.to_string(), 1.0);

        // Convert to requested base if needed
        let rates = if base == INTERNAL_BASE {
            full_rates
        } else {
            Self::convert_base_currency(&full_rates, INTERNAL_BASE, base)?
        };

        // Filter by symbols if specified
        let mut rates = rates;
        if let Some(symbols) = symbols {
            rates.retain(|k, _| symbols.contains(k));
        }

        // Apply amount multiplier and rounding
        let rates: HashMap<String, f64> = rates
            .into_iter()
            .map(|(k, v)| (k, Self::round_rate(v * amount)))
            .collect();

        Ok(RatesResponse {
            amount,
            base: base.to_string(),
            date,
            rates,
        })
    }

    /// Get rates for a date range (time series)
    pub async fn get_time_series(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        base: &str,
        symbols: Option<&[String]>,
        amount: f64,
    ) -> Result<TimeSeriesResponse> {
        // All rates are stored internally as USD-based
        let usd_rates = self
            .repository
            .get_rates_for_range(start, end, INTERNAL_BASE, None)
            .await?;

        if usd_rates.is_empty() {
            return Err(AppError::NoDataAvailable);
        }

        // Convert each day's rates to requested base
        let mut all_rates: HashMap<NaiveDate, HashMap<String, f64>> = HashMap::new();

        for (date, mut rates) in usd_rates {
            // Add USD = 1.0 for conversion
            rates.insert(INTERNAL_BASE.to_string(), 1.0);

            let converted = if base == INTERNAL_BASE {
                rates
            } else {
                Self::convert_base_currency(&rates, INTERNAL_BASE, base)?
            };

            all_rates.insert(date, converted);
        }

        // Filter and apply amount
        let rates: HashMap<NaiveDate, HashMap<String, f64>> = all_rates
            .into_iter()
            .map(|(date, mut day_rates)| {
                // Filter by symbols
                if let Some(symbols) = symbols {
                    day_rates.retain(|k, _| symbols.contains(k));
                }

                // Apply amount and round
                let rates: HashMap<String, f64> = day_rates
                    .into_iter()
                    .map(|(k, v)| (k, Self::round_rate(v * amount)))
                    .collect();

                (date, rates)
            })
            .collect();

        Ok(TimeSeriesResponse {
            amount,
            base: base.to_string(),
            start_date: start,
            end_date: end,
            rates,
        })
    }

    /// Get available currencies
    pub async fn get_currencies(&self) -> Result<HashMap<String, String>> {
        self.repository.get_currencies(None).await
    }

    /// Get providers info for health check
    pub async fn get_providers_info(&self) -> Result<Vec<crate::models::ProviderInfo>> {
        let mut infos = Vec::new();

        for provider in self.providers.all() {
            let last_sync = self.repository.get_last_sync(provider.name()).await?;
            let count = self.repository.get_rates_count(provider.name()).await?;

            infos.push(crate::models::ProviderInfo {
                name: provider.name().to_string(),
                enabled: true,
                last_sync,
                currencies_count: count as usize,
            });
        }

        Ok(infos)
    }
}
