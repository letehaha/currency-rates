# USD Currency Rates API

A self-hosted currency exchange rates API with historical data back to 1999. Fetches from ECB and NBU, stores in SQLite, serves via REST.

## Features

- **38 currencies** from ECB and NBU with history since 1999
- **USD-based** — all rates normalized to USD
- **Self-contained** — SQLite storage, no external dependencies
- **Auto-sync** — scheduled updates via cron
- **Fast startup** — seeds from bundled historical data

## Quick Start

Add to your `docker-compose.yml` as simple as this:

```yaml
services:
  currency-rates-api:
    image: letehaha/currency-rates-api
    volumes:
      - currency-data:/app/data

volumes:
  currency-data:
```

```bash
docker compose up -d
```

API will be available at `http://currency-rates-api:8080`. Read [Configuration](#configuration) for more details.

## API

### Get rates

```bash
# Latest rates
GET /latest
GET /latest?from=EUR&to=USD,GBP&amount=100

# Historical rates
GET /2025-11-27
GET /2025-11-27?from=EUR&to=USD,GBP

# Time series
GET /2025-11-01..2025-11-27
GET /2025-11-01..2025-11-27?from=EUR&to=USD
```

### Response format

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

### Other endpoints

| Endpoint          | Description                                    |
| ----------------- | ---------------------------------------------- |
| `GET /currencies` | List all currencies with available date ranges |
| `GET /health`     | Health check                                   |
| `POST /sync`      | Trigger manual sync (all providers)            |
| `POST /sync/ecb`  | Sync specific provider                         |

### Query parameters

| Parameter | Description                         | Example          |
| --------- | ----------------------------------- | ---------------- |
| `from`    | Base currency                       | `from=EUR`       |
| `to`      | Target currencies (comma-separated) | `to=USD,GBP,JPY` |
| `amount`  | Amount to convert                   | `amount=100`     |

## Configuration

| Variable           | Default                             | Description                             |
| ------------------ | ----------------------------------- | --------------------------------------- |
| `DATABASE_URL`     | `sqlite:currency_rates.db?mode=rwc` | SQLite path                             |
| `HOST`             | `0.0.0.0`                           | Server host                             |
| `PORT`             | `8080`                              | Server port                             |
| `DEFAULT_API_BASE` | `USD`                               | Default base currency\*                 |
| `SEED_ON_STARTUP`  | `true`                              | Seed from bundled files if DB empty\*\* |
| `SYNC_ON_STARTUP`  | `true`                              | Sync latest rates on startup            |
| `SYNC_CRON`        | `0 0 16 * * *`                      | Cron schedule (default: 4 PM UTC)       |

> \* All rates stored internally as USD-based. This only affects the default `from` parameter.

> \*\* Seeding loads historical data locally, so subsequent sync only fetches ~2 weeks instead of 25+ years.

## Supported Currencies

**38 currencies** from two providers:

| Provider | Currencies                                                                                                                                           | Since  |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ------ |
| ECB      | AUD, BGN, BRL, CAD, CHF, CNY, CZK, DKK, EUR, GBP, HKD, HUF, IDR, ILS, INR, ISK, JPY, KRW, MXN, MYR, NOK, NZD, PHP, PLN, RON, SEK, SGD, THB, TRY, ZAR | 1999\* |
| NBU      | EGP, GEL, KZT, LBP, MDL, SAR, UAH, VND                                                                                                               | 1999   |

> \* Some ECB currencies added later (BGN 2000, CNY/TRY 2005, BRL/MXN 2008, ILS 2011)

## Design Notes

**No data for "today" until synced** — The API returns no data for dates that haven't been synced yet. This is intentional; implement fallback logic in your application if needed.

**Cron frequency** — Default is once daily at 4 PM UTC. Consider running more frequently since some banks (like NBU) don't have fixed publishing times:

```bash
SYNC_CRON="0 0 */4 * * *"  # Every 4 hours
```

---

## Development

### Prerequisites

- Rust 1.85+
- Copy `.env.example` to `.env`

### Commands

```bash
cargo run                     # Run server
cargo run --release --bin seed  # Seed database manually
cargo test                    # Run tests
cargo clippy                  # Lint
cargo fmt                     # Format
```

### How it works

1. Creates SQLite database
2. Seeds from bundled historical files (if empty)
3. Syncs latest rates from APIs
4. Serves HTTP on `http://0.0.0.0:8080`

### Data providers

| Provider | Base | Update     | Notes                                            |
| -------- | ---- | ---------- | ------------------------------------------------ |
| ECB      | EUR  | ~16:00 CET | No weekends/holidays (gaps filled automatically) |
| NBU      | UAH  | Daily      | Exact sync time is unknown                       |

All rates converted to USD internally.

### Adding a provider

1. Implement the `Provider` trait in `src/providers/`:

```rust
#[async_trait]
impl Provider for MyProvider {
    fn name(&self) -> &str { "my_provider" }
    fn native_base_currency(&self) -> &str { "USD" }

    async fn fetch_latest(&self) -> Result<DailyRates> { /* ... */ }
    async fn fetch_date(&self, date: NaiveDate) -> Result<DailyRates> { /* ... */ }
    async fn fetch_full_history(&self) -> Result<Vec<DailyRates>> { /* ... */ }
}
```

2. Register in `src/main.rs`:

```rust
providers.register(MyProvider::new());
```

### Database schema

```sql
CREATE TABLE exchange_rates (
    id INTEGER PRIMARY KEY,
    date TEXT NOT NULL,
    base_currency TEXT NOT NULL,
    target_currency TEXT NOT NULL,
    rate REAL NOT NULL,
    provider TEXT NOT NULL,
    UNIQUE(date, base_currency, target_currency, provider)
);

CREATE TABLE currencies (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider TEXT NOT NULL
);

CREATE TABLE sync_log (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    synced_at TEXT DEFAULT CURRENT_TIMESTAMP,
    records_count INTEGER,
    status TEXT
);
```

## License

MIT
