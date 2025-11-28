use async_trait::async_trait;
use chrono::NaiveDate;
use quick_xml::de::from_str;
use serde::Deserialize;
use std::collections::HashMap;

use crate::error::{AppError, Result};
use crate::models::{Currency, DailyRates};
use crate::providers::Provider;

const ECB_DAILY_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const ECB_HIST_90D_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist-90d.xml";
const ECB_HIST_FULL_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist.xml";

/// ECB XML structure for parsing
#[derive(Debug, Deserialize)]
#[serde(rename = "Envelope")]
struct EcbEnvelope {
    #[serde(rename = "Cube")]
    cube: EcbOuterCube,
}

#[derive(Debug, Deserialize)]
struct EcbOuterCube {
    #[serde(rename = "Cube", default)]
    cubes: Vec<EcbTimeCube>,
}

#[derive(Debug, Deserialize)]
struct EcbTimeCube {
    #[serde(rename = "@time")]
    time: String,
    #[serde(rename = "Cube", default)]
    rates: Vec<EcbRateCube>,
}

#[derive(Debug, Deserialize)]
struct EcbRateCube {
    #[serde(rename = "@currency")]
    currency: String,
    #[serde(rename = "@rate")]
    rate: f64,
}

/// European Central Bank provider
/// Provides EUR-based exchange rates
pub struct EcbProvider {
    client: reqwest::Client,
}

impl EcbProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    async fn fetch_and_parse(&self, url: &str) -> Result<Vec<DailyRates>> {
        let response = self.client.get(url).send().await?;
        let xml_text = response.text().await?;

        let rates = self.parse_xml(&xml_text)?;
        Ok(super::fill_gaps(rates, self.name()))
    }

    fn parse_xml(&self, xml: &str) -> Result<Vec<DailyRates>> {
        let envelope: EcbEnvelope = from_str(xml)?;

        let mut results = Vec::new();

        for time_cube in envelope.cube.cubes {
            let date = NaiveDate::parse_from_str(&time_cube.time, "%Y-%m-%d")?;

            // First, collect EUR-based rates
            let mut eur_rates: HashMap<String, f64> = HashMap::new();
            eur_rates.insert("EUR".to_string(), 1.0);

            for rate_cube in time_cube.rates {
                eur_rates.insert(rate_cube.currency, rate_cube.rate);
            }

            // Get EUR/USD rate (how many USD per 1 EUR)
            let eur_usd = match eur_rates.get("USD") {
                Some(&rate) => rate,
                None => {
                    tracing::warn!("EUR/USD rate not found for date {}, skipping", date);
                    continue;
                }
            };

            // Convert all rates to USD-based: USD/XXX = EUR/XXX / EUR/USD
            let mut usd_rates: HashMap<String, f64> = HashMap::new();
            usd_rates.insert("USD".to_string(), 1.0);

            for (currency, eur_rate) in eur_rates {
                if currency == "USD" {
                    continue; // Already added as 1.0
                }
                // USD/XXX = EUR/XXX / EUR/USD
                let usd_rate = eur_rate / eur_usd;
                usd_rates.insert(currency, usd_rate);
            }

            results.push(DailyRates {
                date,
                base_currency: "USD".to_string(),
                rates: usd_rates,
                provider: self.name().to_string(),
            });
        }

        Ok(results)
    }
}

impl Default for EcbProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for EcbProvider {
    fn name(&self) -> &str {
        "ecb"
    }

    fn description(&self) -> &str {
        "European Central Bank - Daily EUR reference rates"
    }

    async fn supported_currencies(&self) -> Result<Vec<Currency>> {
        // ECB supports these currencies against EUR
        Ok(vec![
            Currency {
                code: "EUR".to_string(),
                name: "Euro".to_string(),
            },
            Currency {
                code: "USD".to_string(),
                name: "US Dollar".to_string(),
            },
            Currency {
                code: "JPY".to_string(),
                name: "Japanese Yen".to_string(),
            },
            Currency {
                code: "BGN".to_string(),
                name: "Bulgarian Lev".to_string(),
            },
            Currency {
                code: "CZK".to_string(),
                name: "Czech Koruna".to_string(),
            },
            Currency {
                code: "DKK".to_string(),
                name: "Danish Krone".to_string(),
            },
            Currency {
                code: "GBP".to_string(),
                name: "British Pound".to_string(),
            },
            Currency {
                code: "HUF".to_string(),
                name: "Hungarian Forint".to_string(),
            },
            Currency {
                code: "PLN".to_string(),
                name: "Polish Zloty".to_string(),
            },
            Currency {
                code: "RON".to_string(),
                name: "Romanian Leu".to_string(),
            },
            Currency {
                code: "SEK".to_string(),
                name: "Swedish Krona".to_string(),
            },
            Currency {
                code: "CHF".to_string(),
                name: "Swiss Franc".to_string(),
            },
            Currency {
                code: "ISK".to_string(),
                name: "Icelandic Krona".to_string(),
            },
            Currency {
                code: "NOK".to_string(),
                name: "Norwegian Krone".to_string(),
            },
            Currency {
                code: "TRY".to_string(),
                name: "Turkish Lira".to_string(),
            },
            Currency {
                code: "AUD".to_string(),
                name: "Australian Dollar".to_string(),
            },
            Currency {
                code: "BRL".to_string(),
                name: "Brazilian Real".to_string(),
            },
            Currency {
                code: "CAD".to_string(),
                name: "Canadian Dollar".to_string(),
            },
            Currency {
                code: "CNY".to_string(),
                name: "Chinese Yuan".to_string(),
            },
            Currency {
                code: "HKD".to_string(),
                name: "Hong Kong Dollar".to_string(),
            },
            Currency {
                code: "IDR".to_string(),
                name: "Indonesian Rupiah".to_string(),
            },
            Currency {
                code: "ILS".to_string(),
                name: "Israeli Shekel".to_string(),
            },
            Currency {
                code: "INR".to_string(),
                name: "Indian Rupee".to_string(),
            },
            Currency {
                code: "KRW".to_string(),
                name: "South Korean Won".to_string(),
            },
            Currency {
                code: "MXN".to_string(),
                name: "Mexican Peso".to_string(),
            },
            Currency {
                code: "MYR".to_string(),
                name: "Malaysian Ringgit".to_string(),
            },
            Currency {
                code: "NZD".to_string(),
                name: "New Zealand Dollar".to_string(),
            },
            Currency {
                code: "PHP".to_string(),
                name: "Philippine Peso".to_string(),
            },
            Currency {
                code: "SGD".to_string(),
                name: "Singapore Dollar".to_string(),
            },
            Currency {
                code: "THB".to_string(),
                name: "Thai Baht".to_string(),
            },
            Currency {
                code: "ZAR".to_string(),
                name: "South African Rand".to_string(),
            },
        ])
    }

    async fn fetch_latest(&self) -> Result<DailyRates> {
        let rates = self.fetch_and_parse(ECB_DAILY_URL).await?;
        rates
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Provider("No rates found in ECB response".to_string()))
    }

    async fn fetch_date(&self, date: NaiveDate) -> Result<DailyRates> {
        // ECB doesn't support fetching by specific date via URL
        // We need to fetch historical data and filter
        let rates = self.fetch_and_parse(ECB_HIST_90D_URL).await?;

        rates
            .into_iter()
            .find(|r| r.date == date)
            .ok_or_else(|| AppError::NoDataAvailable)
    }

    async fn fetch_range(&self, start: NaiveDate, end: NaiveDate) -> Result<Vec<DailyRates>> {
        // Determine which endpoint to use based on date range
        let today = chrono::Utc::now().date_naive();
        let days_ago_90 = today - chrono::Duration::days(90);

        let url = if start >= days_ago_90 {
            ECB_HIST_90D_URL
        } else {
            ECB_HIST_FULL_URL
        };

        let all_rates = self.fetch_and_parse(url).await?;

        Ok(all_rates
            .into_iter()
            .filter(|r| r.date >= start && r.date <= end)
            .collect())
    }

    async fn fetch_full_history(&self) -> Result<Vec<DailyRates>> {
        self.fetch_and_parse(ECB_HIST_FULL_URL).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
    <gesmes:subject>Reference rates</gesmes:subject>
    <gesmes:Sender>
        <gesmes:name>European Central Bank</gesmes:name>
    </gesmes:Sender>
    <Cube>
        <Cube time='2025-11-27'>
            <Cube currency='USD' rate='1.0586'/>
            <Cube currency='JPY' rate='158.11'/>
        </Cube>
    </Cube>
</gesmes:Envelope>"#;

        let provider = EcbProvider::new();
        let rates = provider.parse_xml(xml).unwrap();

        assert_eq!(rates.len(), 1);
        assert_eq!(
            rates[0].date,
            NaiveDate::from_ymd_opt(2025, 11, 27).unwrap()
        );
        // Base currency should now be USD
        assert_eq!(rates[0].base_currency, "USD");
        // USD should be 1.0
        assert_eq!(rates[0].rates.get("USD"), Some(&1.0));
        // EUR should be 1/1.0586 ≈ 0.9446
        let eur_rate = rates[0].rates.get("EUR").unwrap();
        assert!((eur_rate - (1.0 / 1.0586)).abs() < 0.0001);
        // JPY should be 158.11/1.0586 ≈ 149.36
        let jpy_rate = rates[0].rates.get("JPY").unwrap();
        assert!((jpy_rate - (158.11 / 1.0586)).abs() < 0.01);
    }
}
