# USD Currency Rates API

A Rust-based currency exchange rates API server that fetches rates from multiple providers and stores them in SQLite.

## Features

- 🌍 **Multiple Providers**: ECB (European Central Bank), NBU (National Bank of Ukraine)
- 💱 **USD Base Currency**: All rates converted to USD base by default
- 🗄️ **SQLite Storage**: Lightweight, file-based database
- 📅 **Historical Data**: Full history from ECB (since 1999)
- ⏰ **Automatic Sync**: Scheduled updates via cron
- 🔌 **Extensible**: Easy to add new data providers

## Supported Currencies

**Total: 38 unique currencies** (30 from ECB, 8 from NBU)

### From 1999-01-04 (26 currencies):

**ECB (18):** AUD, CAD, CHF, CZK, DKK, EUR, GBP, HKD, HUF, ISK, JPY, KRW, NOK, NZD, PLN, SEK, SGD, ZAR

**NBU (8):** EGP, GEL, KZT, LBP, MDL, SAR, UAH, VND

### Added later (12 currencies):

- **2000-07-19:** BGN _(ECB)_
- **2005-01-03:** TRY _(ECB)_
- **2005-04-01:** CNY, IDR, MYR, PHP, THB _(ECB)_
- **2005-07-01:** RON _(ECB)_
- **2008-01-02:** BRL, MXN _(ECB)_
- **2009-01-02:** INR _(ECB)_
- **2011-01-03:** ILS _(ECB)_

## Quick Start

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- fulfilled `.env` file from example

### Run

```bash
# Seed the DB
cargo run --release --bin seed

# Run the server with debug logging
RUST_LOG=debug cargo run
```

The server will:

1. Create the SQLite database
2. Seed DB with available historical data (up to 2025-11-27)
3. Fetch historical rates from all providers up to today
4. Start the HTTP server on `http://0.0.0.0:8080`

## Database Seeding

To avoid fetching all historical data from APIs every time you start the server with a fresh database, you can pre-seed the database with historical data.

### Using Seed Files

1. **Obtain seed data files** (or use the provided ones in `seed_data/`):

   - `ecb-full-hist.xml` - ECB historical data (from 1999)
   - `nbu-full-hist.json` - NBU historical data (from 1999)

2. **Run the seeder**:

```bash
# Local development
cargo run --release --bin seed

# With custom paths
ECB_SEED_PATH=/path/to/ecb.xml NBU_SEED_PATH=/path/to/nbu.json cargo run --release --bin seed
```

### Docker Seeding

#### One-time seeding before starting the server:

```bash
# Build the image
docker-compose build

# Run the seeder (one-time)
docker-compose run --rm seed-db

# Start the server (with SYNC_ON_STARTUP=false to skip initial sync)
docker-compose up -d currency-rates-api
```

### Benefits

- **Faster startup**: No need to fetch 25+ years of historical data on first run
- **Reduced API calls**: Avoid hitting provider APIs unnecessarily
- **Offline setup**: Pre-populate database without internet connection
- **Reproducible**: Same historical data across environments

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

### Existing providers limitation

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
# Seed the DB
cargo run --release --bin seed

# Run the server with debug logging
RUST_LOG=debug cargo run

# Run tests
cargo test

# Check lints
cargo clippy

# Format code
cargo fmt
```

## Docker

### Quick Start with Docker

```bash
# Build and start the server
docker-compose up -d

# Check logs
docker-compose logs -f currency-rates-api

# Stop
docker-compose down
```

The database is persisted in `./docker-data/` on the host.

## License

MIT
