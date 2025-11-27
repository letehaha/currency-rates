use async_trait::async_trait;
use chrono::NaiveDate;
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::{AppError, Result};
use crate::models::{Currency, DailyRates};
use crate::providers::Provider;

const NBU_BASE_URL: &str = "https://bank.gov.ua/NBUStatService/v1/statdirectory/exchange";
const NBU_BATCH_URL: &str = "https://bank.gov.ua/NBU_Exchange/exchange_site";

/// Currencies to fetch from NBU (base currency is added dynamically)
/// These are regional/unique currencies best sourced from NBU
const NBU_CURRENCIES: &[&str] = &[
    "UAH", // Ukrainian Hryvnia
    "KZT", // Kazakhstani Tenge
    "LBP", // Lebanese Pound
    "MDL", // Moldovan Leu
    "SAR", // Saudi Riyal
    "VND", // Vietnamese Dong
    "EGP", // Egyptian Pound
    "GEL", // Georgian Lari
           // "DZD", // Algerian Dinar. From 2016-11-01
           // "BDT", // Bangladeshi Taka. From 2016-11-01
           // "AED", // UAE Dirham. From 2016-11-01
           // "TND", // Tunisian Dinar. From 2016-11-01
           // "RSD", // Serbian Dinar. From 2016-11-01
           // "AZN", // Azerbaijani Manat. From 2014-04-04
];

/// NBU API response structure for single date
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct NbuRate {
    r030: i32,            // Currency code
    rate: f64,            // Exchange rate per 1 (!) unit
    txt: String,          // Currency name (Ukrainian)
    cc: String,           // Currency code (ISO)
    exchangedate: String, // Date in DD.MM.YYYY format
}

/// NBU batch API response structure
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct NbuBatchRate {
    exchangedate: String,     // Date in DD.MM.YYYY format
    r030: i32,                // Currency code
    cc: String,               // Currency code (ISO)
    txt: Option<String>,      // Currency name (Ukrainian) - can be null
    enname: Option<String>,   // Currency name (English) - can be null for historical dates
    rate: f64,                // Exchange rate
    units: i32,               // Units per rate
    rate_per_unit: f64,       // Exchange rate per unit
    calcdate: Option<String>, // Date in DD.MM.YYYY format - can be null or whitespace
    #[serde(default)]
    group: Option<String>, // Group identifier - may not exist in all responses
}

/// Internal base currency for storage (all providers convert to this)
const INTERNAL_BASE: &str = "USD";

/// National Bank of Ukraine provider
/// Fetches UAH-based rates and converts to USD for internal storage
pub struct NbuProvider {
    client: reqwest::Client,
}

impl NbuProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Get currencies to fetch (USD for conversion + NBU-specific currencies)
    fn currencies_to_fetch() -> Vec<&'static str> {
        let mut currencies: Vec<&str> = vec![INTERNAL_BASE];
        currencies.extend(NBU_CURRENCIES.iter().filter(|&&c| c != INTERNAL_BASE));
        currencies
    }

    fn format_date_for_nbu(date: NaiveDate) -> String {
        date.format("%Y%m%d").to_string()
    }

    fn format_date_for_batch(date: NaiveDate) -> String {
        date.format("%Y%m%d").to_string()
    }

    fn parse_nbu_date(date_str: &str) -> Result<NaiveDate> {
        NaiveDate::parse_from_str(date_str, "%d.%m.%Y").map_err(AppError::DateParse)
    }
}

impl Default for NbuProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for NbuProvider {
    fn name(&self) -> &str {
        "nbu"
    }

    fn description(&self) -> &str {
        "National Bank of Ukraine - Daily UAH reference rates"
    }

    async fn supported_currencies(&self) -> Result<Vec<Currency>> {
        // Fetch latest to get available currencies
        let url = format!("{}?json", NBU_BASE_URL);
        let response = self.client.get(&url).send().await?;
        let rates: Vec<NbuRate> = response.json().await?;

        let mut currencies: Vec<Currency> = rates
            .into_iter()
            .map(|r| Currency {
                code: r.cc,
                name: r.txt,
            })
            .collect();

        // Add UAH as base
        currencies.insert(
            0,
            Currency {
                code: "UAH".to_string(),
                name: "Ukrainian Hryvnia".to_string(),
            },
        );

        Ok(currencies)
    }

    async fn fetch_latest(&self) -> Result<DailyRates> {
        let url = format!("{}?json", NBU_BASE_URL);
        let response = self.client.get(&url).send().await?;
        let nbu_rates: Vec<NbuRate> = response.json().await?;

        if nbu_rates.is_empty() {
            return Err(AppError::Provider("No rates returned from NBU".to_string()));
        }

        let date = Self::parse_nbu_date(&nbu_rates[0].exchangedate)?;

        // First, collect XXX/UAH rates (1 XXX = rate UAH)
        let mut uah_rates: HashMap<String, f64> = HashMap::new();
        for r in &nbu_rates {
            uah_rates.insert(r.cc.clone(), r.rate);
        }

        // Get USD/UAH rate (how many UAH per 1 USD)
        let usd_uah = match uah_rates.get(INTERNAL_BASE) {
            Some(&rate) => rate,
            None => {
                return Err(AppError::Provider(format!(
                    "{}/UAH rate not found in NBU response",
                    INTERNAL_BASE
                )));
            }
        };

        // Convert all rates to USD-based: USD/XXX = USD/UAH / XXX/UAH
        // Example: if 1 USD = 42.30 UAH and 1 AZN = 24.88 UAH
        // Then 1 USD = 42.30 / 24.88 = 1.70 AZN
        let mut usd_rates: HashMap<String, f64> = HashMap::new();
        usd_rates.insert(INTERNAL_BASE.to_string(), 1.0);
        usd_rates.insert("UAH".to_string(), usd_uah); // USD/UAH = how many UAH per 1 USD

        for (currency, uah_rate) in uah_rates {
            if currency == INTERNAL_BASE {
                continue; // Already added as 1.0
            }
            // USD/XXX = USD/UAH / XXX/UAH
            let usd_rate = usd_uah / uah_rate;
            usd_rates.insert(currency, usd_rate);
        }

        Ok(DailyRates {
            date,
            base_currency: INTERNAL_BASE.to_string(),
            rates: usd_rates,
            provider: self.name().to_string(),
        })
    }

    async fn fetch_date(&self, date: NaiveDate) -> Result<DailyRates> {
        let date_str = Self::format_date_for_nbu(date);
        let url = format!("{}?date={}&json", NBU_BASE_URL, date_str);

        let response = self.client.get(&url).send().await?;
        let nbu_rates: Vec<NbuRate> = response.json().await?;

        if nbu_rates.is_empty() {
            return Err(AppError::NoDataAvailable);
        }

        // First, collect XXX/UAH rates
        let mut uah_rates: HashMap<String, f64> = HashMap::new();
        for r in &nbu_rates {
            uah_rates.insert(r.cc.clone(), r.rate);
        }

        // Get USD/UAH rate
        let usd_uah = match uah_rates.get(INTERNAL_BASE) {
            Some(&rate) => rate,
            None => {
                return Err(AppError::Provider(format!(
                    "{}/UAH rate not found in NBU response",
                    INTERNAL_BASE
                )));
            }
        };

        // Convert all rates to USD-based: USD/XXX = USD/UAH / XXX/UAH
        let mut usd_rates: HashMap<String, f64> = HashMap::new();
        usd_rates.insert(INTERNAL_BASE.to_string(), 1.0);
        usd_rates.insert("UAH".to_string(), usd_uah);

        for (currency, uah_rate) in uah_rates {
            if currency == INTERNAL_BASE {
                continue;
            }
            let usd_rate = usd_uah / uah_rate;
            usd_rates.insert(currency, usd_rate);
        }

        Ok(DailyRates {
            date,
            base_currency: INTERNAL_BASE.to_string(),
            rates: usd_rates,
            provider: self.name().to_string(),
        })
    }

    async fn fetch_range(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<DailyRates>> {
        let start_str = Self::format_date_for_batch(start);
        let end_str = Self::format_date_for_batch(end);
        let currencies = Self::currencies_to_fetch();

        // Collect all XXX/UAH rates by date first
        let mut uah_rates_by_date: HashMap<NaiveDate, HashMap<String, f64>> = HashMap::new();

        for currency in currencies {
            let url = format!(
                "{}?start={}&end={}&valcode={}&sort=exchangedate&order=asc&json",
                NBU_BATCH_URL,
                start_str,
                end_str,
                currency.to_lowercase()
            );

            tracing::info!("Fetching NBU batch for {}: {}", currency, url);

            match self.client.get(&url).send().await {
                Ok(response) => match response.json::<Vec<NbuBatchRate>>().await {
                    Ok(batch_rates) => {
                        for batch_rate in batch_rates {
                            let date = match Self::parse_nbu_date(&batch_rate.exchangedate) {
                                Ok(d) => d,
                                Err(_) => continue,
                            };

                            uah_rates_by_date
                                .entry(date)
                                .or_default()
                                .insert(batch_rate.cc.to_uppercase(), batch_rate.rate_per_unit);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse NBU batch response for {}: {}",
                            currency,
                            e
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to fetch NBU batch for {}: {}", currency, e);
                }
            }

            // Small delay between currency fetches
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        // Convert to USD-based DailyRates
        let mut results: Vec<DailyRates> = Vec::new();

        for (date, uah_rates) in uah_rates_by_date {
            // Get USD/UAH rate for this date
            let usd_uah = match uah_rates.get(INTERNAL_BASE) {
                Some(&rate) => rate,
                None => {
                    tracing::warn!(
                        "{}/UAH rate not found for date {}, skipping",
                        INTERNAL_BASE,
                        date
                    );
                    continue;
                }
            };

            // Convert all rates to USD-based: USD/XXX = USD/UAH / XXX/UAH
            let mut usd_rates: HashMap<String, f64> = HashMap::new();
            usd_rates.insert(INTERNAL_BASE.to_string(), 1.0);
            usd_rates.insert("UAH".to_string(), usd_uah);

            for (currency, uah_rate) in uah_rates {
                if currency == INTERNAL_BASE {
                    continue;
                }
                let usd_rate = usd_uah / uah_rate;
                usd_rates.insert(currency, usd_rate);
            }

            results.push(DailyRates {
                date,
                base_currency: INTERNAL_BASE.to_string(),
                rates: usd_rates,
                provider: self.name().to_string(),
            });
        }

        // Sort by date
        results.sort_by_key(|r| r.date);

        tracing::info!("Fetched {} days of NBU data via batch API", results.len());
        Ok(results)
    }

    async fn fetch_full_history(&self) -> Result<Vec<DailyRates>> {
        // Fetch full history from 1999-01-04 (same as ECB start date)
        let end = chrono::Utc::now().date_naive();
        let start = NaiveDate::from_ymd_opt(1999, 1, 4).unwrap();

        tracing::info!("Fetching NBU history from {} to {}", start, end);
        self.fetch_range(start, end).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_date_formatting() {
        let date = NaiveDate::from_ymd_opt(2025, 11, 27).unwrap();
        assert_eq!(NbuProvider::format_date_for_nbu(date), "20251127");
    }

    #[test]
    fn test_parse_nbu_date() {
        let result = NbuProvider::parse_nbu_date("27.11.2025").unwrap();
        assert_eq!(result, NaiveDate::from_ymd_opt(2025, 11, 27).unwrap());
    }
}
