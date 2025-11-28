use chrono::NaiveDate;
use sqlx::{sqlite::SqlitePool, FromRow, Row};
use std::collections::HashMap;

use crate::error::Result;
use crate::models::{DailyRates, ExchangeRate};

/// Database row for exchange rates
#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct RateRow {
    id: i64,
    date: String,
    base_currency: String,
    target_currency: String,
    rate: f64,
    provider: String,
}

/// Database row for currencies
#[derive(Debug, FromRow)]
struct CurrencyRow {
    code: String,
    name: String,
}

/// Repository for exchange rate data
#[derive(Clone)]
pub struct RatesRepository {
    pool: SqlitePool,
}

impl RatesRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Initialize the database schema
    pub async fn init(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS exchange_rates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                base_currency TEXT NOT NULL,
                target_currency TEXT NOT NULL,
                rate REAL NOT NULL,
                provider TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(date, base_currency, target_currency, provider)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_rates_date ON exchange_rates(date)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_rates_base ON exchange_rates(base_currency)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_rates_provider ON exchange_rates(provider)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS currencies (
                code TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                UNIQUE(code, provider)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                synced_at TEXT DEFAULT CURRENT_TIMESTAMP,
                records_count INTEGER,
                status TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Store a single exchange rate
    pub async fn store_rate(&self, rate: &ExchangeRate) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO exchange_rates (date, base_currency, target_currency, rate, provider)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(rate.date.to_string())
        .bind(&rate.base_currency)
        .bind(&rate.target_currency)
        .bind(rate.rate)
        .bind(&rate.provider)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Store daily rates batch
    pub async fn store_daily_rates(&self, daily: &DailyRates) -> Result<()> {
        for (currency, rate) in &daily.rates {
            if currency == &daily.base_currency {
                continue; // Skip base currency (rate would be 1.0)
            }

            let exchange_rate = ExchangeRate {
                date: daily.date,
                base_currency: daily.base_currency.clone(),
                target_currency: currency.clone(),
                rate: *rate,
                provider: daily.provider.clone(),
            };

            self.store_rate(&exchange_rate).await?;
        }

        Ok(())
    }

    /// Store multiple daily rates (bulk insert with single transaction)
    pub async fn store_daily_rates_batch(&self, rates: &[DailyRates]) -> Result<usize> {
        let mut count = 0;

        // Use a single transaction for all inserts
        let mut tx = self.pool.begin().await?;

        for daily in rates {
            for (currency, rate) in &daily.rates {
                if currency == &daily.base_currency {
                    continue; // Skip base currency (rate would be 1.0)
                }

                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO exchange_rates (date, base_currency, target_currency, rate, provider)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                )
                .bind(daily.date.to_string())
                .bind(&daily.base_currency)
                .bind(currency)
                .bind(rate)
                .bind(&daily.provider)
                .execute(&mut *tx)
                .await?;

                count += 1;
            }

            // Log progress every 100 days
            if count % 1000 == 0 {
                tracing::info!("Inserted {} records so far...", count);
            }
        }

        // Commit the transaction
        tx.commit().await?;

        Ok(count)
    }

    /// Get the latest available date for a provider
    pub async fn get_latest_date(&self, provider: Option<&str>) -> Result<Option<NaiveDate>> {
        let query = match provider {
            Some(p) => {
                sqlx::query("SELECT MAX(date) as max_date FROM exchange_rates WHERE provider = ?")
                    .bind(p)
            }
            None => sqlx::query("SELECT MAX(date) as max_date FROM exchange_rates"),
        };

        let row = query.fetch_optional(&self.pool).await?;

        if let Some(row) = row {
            let date_str: Option<String> = row.get("max_date");
            if let Some(date_str) = date_str {
                let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?;
                return Ok(Some(date));
            }
        }

        Ok(None)
    }

    /// Get rates for a specific date
    pub async fn get_rates_for_date(
        &self,
        date: NaiveDate,
        base_currency: &str,
        provider: Option<&str>,
    ) -> Result<HashMap<String, f64>> {
        let date_str = date.to_string();

        let rows: Vec<RateRow> = match provider {
            Some(p) => {
                sqlx::query_as(
                    r#"
                    SELECT id, date, base_currency, target_currency, rate, provider
                    FROM exchange_rates
                    WHERE date = ? AND base_currency = ? AND provider = ?
                    "#,
                )
                .bind(&date_str)
                .bind(base_currency)
                .bind(p)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT id, date, base_currency, target_currency, rate, provider
                    FROM exchange_rates
                    WHERE date = ? AND base_currency = ?
                    "#,
                )
                .bind(&date_str)
                .bind(base_currency)
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut rates: HashMap<String, f64> = HashMap::new();
        for row in rows {
            rates.insert(row.target_currency, row.rate);
        }

        Ok(rates)
    }

    /// Get rates for a date range
    pub async fn get_rates_for_range(
        &self,
        start: NaiveDate,
        end: NaiveDate,
        base_currency: &str,
        provider: Option<&str>,
    ) -> Result<HashMap<NaiveDate, HashMap<String, f64>>> {
        let start_str = start.to_string();
        let end_str = end.to_string();

        let rows: Vec<RateRow> = match provider {
            Some(p) => {
                sqlx::query_as(
                    r#"
                    SELECT id, date, base_currency, target_currency, rate, provider
                    FROM exchange_rates
                    WHERE date >= ? AND date <= ? AND base_currency = ? AND provider = ?
                    ORDER BY date
                    "#,
                )
                .bind(&start_str)
                .bind(&end_str)
                .bind(base_currency)
                .bind(p)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT id, date, base_currency, target_currency, rate, provider
                    FROM exchange_rates
                    WHERE date >= ? AND date <= ? AND base_currency = ?
                    ORDER BY date
                    "#,
                )
                .bind(&start_str)
                .bind(&end_str)
                .bind(base_currency)
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut results: HashMap<NaiveDate, HashMap<String, f64>> = HashMap::new();

        for row in rows {
            let date = NaiveDate::parse_from_str(&row.date, "%Y-%m-%d")?;
            results
                .entry(date)
                .or_default()
                .insert(row.target_currency, row.rate);
        }

        Ok(results)
    }

    /// Get all available currencies from exchange_rates (source of truth)
    pub async fn get_currencies(&self, provider: Option<&str>) -> Result<HashMap<String, String>> {
        let rows: Vec<CurrencyRow> = match provider {
            Some(p) => {
                sqlx::query_as(
                    r#"
                    SELECT DISTINCT er.target_currency as code,
                           COALESCE(c.name, er.target_currency) as name
                    FROM exchange_rates er
                    LEFT JOIN currencies c ON er.target_currency = c.code
                    WHERE er.provider = ?
                    "#,
                )
                .bind(p)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT DISTINCT er.target_currency as code,
                           COALESCE(c.name, er.target_currency) as name
                    FROM exchange_rates er
                    LEFT JOIN currencies c ON er.target_currency = c.code
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut currencies: HashMap<String, String> = HashMap::new();
        for row in rows {
            currencies.insert(row.code, row.name);
        }

        Ok(currencies)
    }

    /// Store currencies
    pub async fn store_currencies(
        &self,
        currencies: &[(String, String)],
        provider: &str,
    ) -> Result<()> {
        for (code, name) in currencies {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO currencies (code, name, provider)
                VALUES (?, ?, ?)
                "#,
            )
            .bind(code)
            .bind(name)
            .bind(provider)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Log a sync operation
    pub async fn log_sync(&self, provider: &str, records_count: usize, status: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO sync_log (provider, records_count, status)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(provider)
        .bind(records_count as i64)
        .bind(status)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get last sync time for a provider
    pub async fn get_last_sync(&self, provider: &str) -> Result<Option<String>> {
        let row = sqlx::query(
            r#"
            SELECT synced_at FROM sync_log
            WHERE provider = ? AND status = 'success'
            ORDER BY synced_at DESC
            LIMIT 1
            "#,
        )
        .bind(provider)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.get("synced_at")))
    }

    /// Get count of rates per provider
    pub async fn get_rates_count(&self, provider: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) as count FROM exchange_rates WHERE provider = ?")
            .bind(provider)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.get("count"))
    }
}
