use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::models::DailyRates;

/// NBU batch API response structure (from seed file)
#[derive(Debug, Deserialize)]
struct NbuBatchRate {
    exchangedate: String,
    cc: String,
    rate_per_unit: f64,
}

/// Parse NBU seed data JSON file
/// Format: { "USD": [...], "KZT": [...], ... }
pub fn parse_nbu_seed_file(path: &Path) -> Result<Vec<DailyRates>> {
    tracing::info!("Parsing NBU seed file: {}", path.display());

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read NBU seed file: {}", path.display()))?;

    let data: HashMap<String, Vec<NbuBatchRate>> =
        serde_json::from_str(&content).context("Failed to parse NBU seed JSON")?;

    // Organize by date
    let mut by_date: HashMap<NaiveDate, HashMap<String, f64>> = HashMap::new();

    for (_currency, rates) in data {
        for rate in rates {
            let date = NaiveDate::parse_from_str(&rate.exchangedate, "%d.%m.%Y")
                .with_context(|| format!("Failed to parse NBU date: {}", rate.exchangedate))?;

            by_date
                .entry(date)
                .or_default()
                .insert(rate.cc.to_uppercase(), rate.rate_per_unit);
        }
    }

    // Convert to DailyRates with USD base (same logic as NBU provider)
    let mut results: Vec<DailyRates> = Vec::new();
    const INTERNAL_BASE: &str = "USD";

    for (date, uah_rates) in by_date {
        // Get USD/UAH rate for this date
        let usd_uah = match uah_rates.get(INTERNAL_BASE) {
            Some(&rate) => rate,
            None => {
                tracing::warn!("USD/UAH rate not found for date {}, skipping", date);
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
            provider: "nbu".to_string(),
        });
    }

    results.sort_by_key(|r| r.date);
    tracing::info!("Parsed {} days of NBU data from seed file", results.len());

    Ok(results)
}

/// Parse ECB seed data XML file
pub fn parse_ecb_seed_file(path: &Path) -> Result<Vec<DailyRates>> {
    tracing::info!("Parsing ECB seed file: {}", path.display());

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read ECB seed file: {}", path.display()))?;

    // Parse the XML directly
    let rates = parse_ecb_xml(&content)?;

    tracing::info!("Parsed {} days of ECB data from seed file", rates.len());
    Ok(rates)
}

use quick_xml::de::from_str;
use serde::Deserialize as XmlDeserialize;

/// ECB XML structure for parsing (same as in ecb.rs)
#[derive(Debug, XmlDeserialize)]
#[serde(rename = "Envelope")]
struct EcbEnvelope {
    #[serde(rename = "Cube")]
    cube: EcbOuterCube,
}

#[derive(Debug, XmlDeserialize)]
struct EcbOuterCube {
    #[serde(rename = "Cube", default)]
    cubes: Vec<EcbTimeCube>,
}

#[derive(Debug, XmlDeserialize)]
struct EcbTimeCube {
    #[serde(rename = "@time")]
    time: String,
    #[serde(rename = "Cube", default)]
    rates: Vec<EcbRateCube>,
}

#[derive(Debug, XmlDeserialize)]
struct EcbRateCube {
    #[serde(rename = "@currency")]
    currency: String,
    #[serde(rename = "@rate")]
    rate: f64,
}

fn parse_ecb_xml(xml: &str) -> Result<Vec<DailyRates>> {
    let envelope: EcbEnvelope = from_str(xml).context("Failed to parse ECB XML")?;

    let mut results = Vec::new();

    for time_cube in envelope.cube.cubes {
        let date = NaiveDate::parse_from_str(&time_cube.time, "%Y-%m-%d")
            .with_context(|| format!("Failed to parse ECB date: {}", time_cube.time))?;

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
            provider: "ecb".to_string(),
        });
    }

    results.sort_by_key(|r| r.date);
    Ok(results)
}

/// Seed the database with data from seed files
pub async fn seed_database(
    repository: &crate::RatesRepository,
    ecb_seed_path: Option<&Path>,
    nbu_seed_path: Option<&Path>,
) -> Result<()> {
    tracing::info!("Starting database seeding...");

    let mut total_records = 0;

    // Seed ECB data
    if let Some(path) = ecb_seed_path {
        if path.exists() {
            tracing::info!("Seeding ECB data from: {}", path.display());
            let rates = parse_ecb_seed_file(path)?;
            let count = repository.store_daily_rates_batch(&rates).await?;
            repository.log_sync("ecb", rates.len(), "seeded").await?;
            total_records += count;
            tracing::info!("Seeded {} ECB records ({} days)", count, rates.len());
        } else {
            tracing::warn!("ECB seed file not found: {}", path.display());
        }
    }

    // Seed NBU data
    if let Some(path) = nbu_seed_path {
        if path.exists() {
            tracing::info!("Seeding NBU data from: {}", path.display());
            let rates = parse_nbu_seed_file(path)?;
            let count = repository.store_daily_rates_batch(&rates).await?;
            repository.log_sync("nbu", rates.len(), "seeded").await?;
            total_records += count;
            tracing::info!("Seeded {} NBU records ({} days)", count, rates.len());
        } else {
            tracing::warn!("NBU seed file not found: {}", path.display());
        }
    }

    tracing::info!(
        "Database seeding completed. Total records: {}",
        total_records
    );
    Ok(())
}
