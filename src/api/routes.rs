use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use super::handlers::{
    get_currencies, get_historical, get_latest, health_check, root, trigger_provider_sync,
    trigger_sync, AppState,
};

/// Create the API router with all routes
pub fn create_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", get(root))
        .route("/latest", get(get_latest))
        .route("/currencies", get(get_currencies))
        .route("/health", get(health_check))
        // Historical/time series endpoint
        .route("/:date_path", get(get_historical))
        // Admin endpoints
        .route("/sync", post(trigger_sync))
        .route("/sync/:provider", post(trigger_provider_sync))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
