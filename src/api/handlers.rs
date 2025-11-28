use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use std::sync::Arc;

use crate::error::{AppError, Result};
use crate::models::{CurrenciesResponse, HealthResponse, RatesResponse};
use crate::service::RatesService;

/// Shared application state
pub struct AppState {
    pub service: RatesService,
    /// Default base currency for API responses when client doesn't specify one
    pub default_api_base: String,
}

/// Query parameters for rate endpoints
#[derive(Debug, Deserialize)]
pub struct RatesQuery {
    /// Amount to convert (default: 1)
    pub amount: Option<f64>,
    /// Base currency (default: configured base, e.g., USD)
    #[serde(rename = "from")]
    pub base: Option<String>,
    /// Target currencies, comma-separated
    #[serde(rename = "to")]
    pub symbols: Option<String>,
}

impl RatesQuery {
    fn parse_symbols(&self) -> Option<Vec<String>> {
        self.symbols.as_ref().map(|s| {
            s.split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
    }
}

/// GET /
/// Returns basic API info
pub async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "USD Currency Rates API",
        "description": "Currency exchange rates API with multiple providers",
        "endpoints": {
            "/currencies": "List supported currencies",
            "/latest": "Get latest rates",
            "/{date}": "Get rates for a specific date (YYYY-MM-DD)",
            "/{start_date}..{end_date}": "Get rates for a date range",
            "/health": "Health check"
        }
    }))
}

/// GET /latest
/// Get the latest exchange rates
pub async fn get_latest(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RatesQuery>,
) -> Result<Json<RatesResponse>> {
    let base = query.base.as_deref().unwrap_or(&state.default_api_base);
    let symbols = query.parse_symbols();
    let amount = query.amount.unwrap_or(1.0);

    let response = state
        .service
        .get_latest(Some(base), symbols.as_deref(), Some(amount))
        .await?;

    Ok(Json(response))
}

/// GET /currencies
/// List all available currencies
pub async fn get_currencies(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CurrenciesResponse>> {
    let currencies = state.service.get_currencies().await?;
    Ok(Json(currencies))
}

/// GET /health
/// Health check endpoint
pub async fn health_check(State(state): State<Arc<AppState>>) -> Result<Json<HealthResponse>> {
    let providers = state.service.get_providers_info().await?;

    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        providers,
    }))
}

/// GET /{date}
/// Get rates for a specific date or date range
/// Supports: YYYY-MM-DD or YYYY-MM-DD..YYYY-MM-DD
pub async fn get_historical(
    State(state): State<Arc<AppState>>,
    Path(date_path): Path<String>,
    Query(query): Query<RatesQuery>,
) -> Result<Json<serde_json::Value>> {
    let base = query.base.as_deref().unwrap_or(&state.default_api_base);
    let symbols = query.parse_symbols();
    let amount = query.amount.unwrap_or(1.0);

    // Check if it's a date range (YYYY-MM-DD..YYYY-MM-DD)
    if date_path.contains("..") {
        let parts: Vec<&str> = date_path.split("..").collect();
        if parts.len() != 2 {
            return Err(AppError::InvalidDate(
                "Invalid date range format. Use YYYY-MM-DD..YYYY-MM-DD".to_string(),
            ));
        }

        let start = parse_date(parts[0])?;
        let end = parse_date(parts[1])?;

        if start > end {
            return Err(AppError::InvalidDate(
                "Start date must be before or equal to end date".to_string(),
            ));
        }

        let response = state
            .service
            .get_time_series(start, end, base, symbols.as_deref(), amount)
            .await?;

        return Ok(Json(serde_json::to_value(response)?));
    }

    // Single date
    let date = parse_date(&date_path)?;
    tracing::debug!("Fetching rates for date: {}, base: {}", date, base);

    let response = state
        .service
        .get_rates_for_date(date, base, symbols.as_deref(), amount)
        .await?;

    tracing::debug!("Got {} rates", response.rates.len());
    Ok(Json(serde_json::to_value(response)?))
}

/// POST /sync
/// Trigger a manual sync (admin endpoint)
pub async fn trigger_sync(State(state): State<Arc<AppState>>) -> Result<Json<serde_json::Value>> {
    state.service.sync_all_providers().await?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Sync completed"
    })))
}

/// POST /sync/{provider}
/// Trigger sync for a specific provider
pub async fn trigger_provider_sync(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let count = state.service.sync_provider(&provider).await?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "provider": provider,
        "records_synced": count
    })))
}

/// Parse date from string, supporting multiple formats
fn parse_date(s: &str) -> Result<NaiveDate> {
    // Try ISO format first (YYYY-MM-DD)
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date);
    }

    // Try compact format (YYYYMMDD)
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y%m%d") {
        return Ok(date);
    }

    Err(AppError::InvalidDate(format!(
        "Invalid date format: {}. Use YYYY-MM-DD",
        s
    )))
}
