use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single exchange rate record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub date: NaiveDate,
    pub base_currency: String,
    pub target_currency: String,
    pub rate: f64,
    pub provider: String,
}

/// Batch of rates for a single date
#[derive(Debug, Clone)]
pub struct DailyRates {
    pub date: NaiveDate,
    pub base_currency: String,
    pub rates: HashMap<String, f64>,
    pub provider: String,
}

/// Currency metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Currency {
    pub code: String,
    pub name: String,
}

/// Response format for /latest and /YYYY-MM-DD endpoints
#[derive(Debug, Serialize, Deserialize)]
pub struct RatesResponse {
    pub amount: f64,
    pub base: String,
    pub date: NaiveDate,
    pub rates: HashMap<String, f64>,
}

/// Response format for time series endpoint
#[derive(Debug, Serialize, Deserialize)]
pub struct TimeSeriesResponse {
    pub amount: f64,
    pub base: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub rates: HashMap<NaiveDate, HashMap<String, f64>>,
}

/// Currency information including date range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_date: Option<NaiveDate>,
}

/// Response format for /currencies endpoint
pub type CurrenciesResponse = HashMap<String, CurrencyInfo>;

/// Provider info for health check
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub enabled: bool,
    pub last_sync: Option<String>,
    pub currencies_count: usize,
}

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub providers: Vec<ProviderInfo>,
}
