use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// All errors the engine can produce.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("validation failed: {0}")]
    ValidationFailed(String),

    #[error("field not allowed: {0}")]
    FieldNotAllowed(String),

    #[error("pre-aggregate would be too large: {0}")]
    PreaggregateToLarge(String),

    #[error("result too large: {0}")]
    ResultTooLarge(String),

    #[error("query timed out")]
    QueryTimeout,

    #[error("query was cancelled")]
    Cancelled,

    #[error("ClickHouse unavailable: {0}")]
    ClickHouseUnavailable(String),

    #[error("model not found: {0}")]
    ModelNotFound(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl EngineError {
    fn code(&self) -> &'static str {
        match self {
            Self::ValidationFailed(_) => "VALIDATION_FAILED",
            Self::FieldNotAllowed(_) => "FIELD_NOT_ALLOWED",
            Self::PreaggregateToLarge(_) => "PREAGGREGATE_TOO_LARGE",
            Self::ResultTooLarge(_) => "RESULT_TOO_LARGE",
            Self::QueryTimeout => "QUERY_TIMEOUT",
            Self::Cancelled => "CANCELLED",
            Self::ClickHouseUnavailable(_) => "CLICKHOUSE_UNAVAILABLE",
            Self::ModelNotFound(_) => "MODEL_NOT_FOUND",
            Self::Internal(_) => "INTERNAL",
        }
    }

    fn retriable(&self) -> bool {
        matches!(self, Self::ClickHouseUnavailable(_))
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::ValidationFailed(_)
            | Self::FieldNotAllowed(_)
            | Self::PreaggregateToLarge(_)
            | Self::ResultTooLarge(_) => StatusCode::BAD_REQUEST,
            Self::QueryTimeout => StatusCode::GATEWAY_TIMEOUT,
            Self::Cancelled => StatusCode::OK, // 200 with cancelled status
            Self::ClickHouseUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::ModelNotFound(_) => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message_user(&self) -> String {
        match self {
            Self::ValidationFailed(m) => m.clone(),
            Self::FieldNotAllowed(f) => format!("Field '{f}' is not available in this model."),
            Self::PreaggregateToLarge(m) => {
                format!("Result would be too large. Narrow filters or remove a dimension. ({m})")
            }
            Self::ResultTooLarge(m) => {
                format!(
                    "Result exceeds Excel cell limit. Narrow filters or remove a dimension. ({m})"
                )
            }
            Self::QueryTimeout => "Query timed out. Try narrowing your date range.".into(),
            Self::Cancelled => "Query was cancelled.".into(),
            Self::ClickHouseUnavailable(_) => {
                "ClickHouse is temporarily unavailable. Please retry.".into()
            }
            Self::ModelNotFound(id) => format!("Model '{id}' not found."),
            Self::Internal(_) => "An internal error occurred. Please contact support.".into(),
        }
    }
}

impl IntoResponse for EngineError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = json!({
            "status": "error",
            "error": {
                "code": self.code(),
                "message_user": self.message_user(),
                "retriable": self.retriable(),
            }
        });
        (status, Json(body)).into_response()
    }
}
