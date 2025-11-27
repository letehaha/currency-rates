use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("HTTP request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("XML parsing error: {0}")]
    XmlParse(#[from] quick_xml::DeError),

    #[error("JSON parsing error: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("Date parsing error: {0}")]
    DateParse(#[from] chrono::ParseError),

    #[error("Invalid date: {0}")]
    InvalidDate(String),

    #[error("Invalid currency: {0}")]
    InvalidCurrency(String),

    #[error("No data available for the requested date")]
    NoDataAvailable,

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Database(e) => {
                tracing::error!("Database error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            AppError::Request(e) => {
                tracing::error!("HTTP request error: {}", e);
                (StatusCode::BAD_GATEWAY, "Failed to fetch external data")
            }
            AppError::XmlParse(e) => {
                tracing::error!("XML parsing error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to parse provider data",
                )
            }
            AppError::JsonParse(e) => {
                tracing::error!("JSON parsing error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to parse provider data",
                )
            }
            AppError::DateParse(_) | AppError::InvalidDate(_) => {
                (StatusCode::BAD_REQUEST, "Invalid date format")
            }
            AppError::InvalidCurrency(_) => (StatusCode::NOT_FOUND, "Currency not found"),
            AppError::NoDataAvailable => (
                StatusCode::NOT_FOUND,
                "No data available for the requested parameters",
            ),
            AppError::Provider(e) => {
                tracing::error!("Provider error: {}", e);
                (StatusCode::BAD_GATEWAY, "Provider error")
            }
            AppError::Config(e) => {
                tracing::error!("Configuration error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Configuration error")
            }
            AppError::Internal(e) => {
                tracing::error!("Internal error: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
        };

        let body = Json(json!({
            "error": message,
            "message": self.to_string()
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
