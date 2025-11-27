# USD Currency Rates API

A Rust-based currency exchange rates API server that fetches rates from multiple providers and stores them in SQLite.

## Features

- 🌍 **Multiple Providers**: ECB (European Central Bank), NBU (National Bank of Ukraine)
- 💱 **USD Base Currency**: All rates converted to USD base by default
- 🗄️ **SQLite Storage**: Lightweight, file-based database
- 📅 **Historical Data**: Full history from ECB (since 1999)
- ⏰ **Automatic Sync**: Scheduled updates via cron
- 🔌 **Extensible**: Easy to add new data providers

Supported currencies:

```
[
"USD", "PHP", "NZD", "MDL", "KZT", "VND", "ILS", "UAH", "MYR", "KRW", "SEK", "JPY", "GEL", "SIT", "RON", "HKD", "SKK", "TRL", "LVL", "PLN", "BGN", "HRK", "EGP", "IDR", "THB", "BRL", "RUB", "INR", "DKK", "CZK", "EEK", "LBP", "SGD", "MXN", "ZAR", "LTL", "SAR", "MTL", "CAD", "CHF", "HUF", "GBP", "TRY", "ISK", "CYP", "ROL", "AUD", "EUR", "NOK", "CNY"
]
```

## Quick Start

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- fulfilled `.env` file from example

### Run

```bash
# Clone and run
cargo run --release
```

The server will:

1. Create the SQLite database
2. Fetch historical rates from all providers
3. Start the HTTP server on `http://0.0.0.0:8080`

## Configuration

Environment variables (or `.env` file):

| Variable           | Default                             | Description                              |
| ------------------ | ----------------------------------- | ---------------------------------------- |
| `DATABASE_URL`     | `sqlite:currency_rates.db?mode=rwc` | SQLite database path                     |
| `HOST`             | `0.0.0.0`                           | Server host                              |
| `PORT`             | `8080`                              | Server port                              |
| `DEFAULT_API_BASE` | `USD`                               | Default base currency for API responses¹ |
| `SYNC_ON_STARTUP`  | `true`                              | Sync rates on startup                    |
| `SYNC_CRON`        | `0 0 16 * * *`                      | Cron schedule for sync (4 PM UTC)        |

¹ All rates are stored internally with USD as the base currency. This setting only affects the default `from` parameter in API responses when not specified by the client.

## API Endpoints

### Get Latest Rates

```bash
GET /latest

# With parameters
GET /latest?from=EUR&to=USD,GBP&amount=100
```

**Response:**

```json
{
  "amount": 1.0,
  "base": "USD",
  "date": "2025-11-27",
  "rates": {
    "EUR": 0.863557,
    "GBP": 0.755683,
    "JPY": 156.3241
  }
}
```

### Get Historical Rates

```bash
GET /2025-11-27
GET /2025-11-27?from=EUR&to=USD,GBP
```

### Get Time Series

```bash
GET /2025-11-01..2025-11-27
GET /2025-11-01..2025-11-27?from=EUR&to=USD
```

**Response:**

```json
{
  "amount": 1.0,
  "base": "USD",
  "start_date": "2025-11-01",
  "end_date": "2025-11-27",
  "rates": {
    "2025-11-01": { "EUR": 0.86, "GBP": 0.75 },
    "2025-11-02": { "EUR": 0.87, "GBP": 0.76 }
  }
}
```

### List Currencies

```bash
GET /currencies
```

**Response:**

```json
{
  "EUR": "Euro",
  "USD": "US Dollar",
  "GBP": "British Pound",
  "UAH": "Ukrainian Hryvnia"
}
```

### Health Check

```bash
GET /health
```

### Manual Sync (Admin)

```bash
POST /sync          # Sync all providers
POST /sync/ecb      # Sync specific provider
```

## Query Parameters

| Parameter | Description                         | Example          |
| --------- | ----------------------------------- | ---------------- |
| `from`    | Base currency                       | `from=EUR`       |
| `to`      | Target currencies (comma-separated) | `to=USD,GBP,JPY` |
| `amount`  | Amount to convert                   | `amount=100`     |

## Data Providers

### ECB (European Central Bank)

- **Base**: EUR
- **Currencies**: 30+ major currencies
- **History**: Since January 1999
- **Update**: Daily at ~16:00 CET

The ECB does not provide data for weekends and holidays, so gaps are filled
automatically when saved to the database.

### NBU (National Bank of Ukraine)

- **Base**: UAH
- **Currencies**: 8+ currencies
- **History**: Available daily
- **Update**: Daily

Since each provider has USD as a non-base currency, the actual USD/XXX rates are
calculated upon data synchronization.

## Adding New Providers

1. Create a new file in `src/providers/`:

```rust
use async_trait::async_trait;
use crate::providers::Provider;
use crate::models::{Currency, DailyRates};
use crate::error::Result;

pub struct MyProvider {
    client: reqwest::Client,
}

#[async_trait]
impl Provider for MyProvider {
    fn name(&self) -> &str { "my_provider" }
    fn description(&self) -> &str { "My Custom Provider" }
    fn native_base_currency(&self) -> &str { "USD" }

    async fn supported_currencies(&self) -> Result<Vec<Currency>> {
        // Implementation
    }

    async fn fetch_latest(&self) -> Result<DailyRates> {
        // Implementation
    }

    async fn fetch_date(&self, date: NaiveDate) -> Result<DailyRates> {
        // Implementation
    }

    async fn fetch_full_history(&self) -> Result<Vec<DailyRates>> {
        // Implementation
    }
}
```

2. Register in `src/main.rs`:

```rust
providers.register(MyProvider::new());
```

## Database Schema

```sql
-- Exchange rates table
CREATE TABLE exchange_rates (
    id INTEGER PRIMARY KEY,
    date TEXT NOT NULL,
    base_currency TEXT NOT NULL,
    target_currency TEXT NOT NULL,
    rate REAL NOT NULL,
    provider TEXT NOT NULL,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(date, base_currency, target_currency, provider)
);

-- Currencies metadata
CREATE TABLE currencies (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL
);

-- Sync log
CREATE TABLE sync_log (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    synced_at TEXT DEFAULT CURRENT_TIMESTAMP,
    records_count INTEGER,
    status TEXT
);
```

## Development

```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Run tests
cargo test

# Check lints
cargo clippy

# Format code
cargo fmt
```

## Docker (Optional)

```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/currency-rates /usr/local/bin/
EXPOSE 8080
CMD ["currency-rates"]
```

## License

MIT
