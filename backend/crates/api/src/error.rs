//! API error handling

use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use billforge_core::Error as CoreError;
use serde::Serialize;
use std::collections::HashMap;
use validator::ValidationErrors;

/// API error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_errors: Option<HashMap<String, Vec<String>>>,
}

/// API error wrapper
pub struct ApiError(pub CoreError);

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        Self(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let body = ErrorResponse {
            error: ErrorBody {
                code: self.0.error_code(),
                message: self.0.to_string(),
                details: None,
                field_errors: None,
            },
        };

        (status, Json(body)).into_response()
    }
}

/// Validation error with field-level details
pub struct ValidationError {
    pub message: String,
    pub field_errors: HashMap<String, Vec<String>>,
}

impl ValidationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            field_errors: HashMap::new(),
        }
    }

    pub fn with_field(mut self, field: impl Into<String>, error: impl Into<String>) -> Self {
        self.field_errors
            .entry(field.into())
            .or_default()
            .push(error.into());
        self
    }

    pub fn from_validator_errors(errors: ValidationErrors) -> Self {
        let mut field_errors = HashMap::new();
        for (field, field_errs) in errors.field_errors() {
            let messages: Vec<String> = field_errs
                .iter()
                .map(|e| {
                    e.message
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| format!("Invalid value for {}", field))
                })
                .collect();
            field_errors.insert(field.to_string(), messages);
        }
        Self {
            message: "Validation failed".to_string(),
            field_errors,
        }
    }
}

impl IntoResponse for ValidationError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            error: ErrorBody {
                code: "VALIDATION_ERROR",
                message: self.message,
                details: None,
                field_errors: Some(self.field_errors),
            },
        };

        (StatusCode::BAD_REQUEST, Json(body)).into_response()
    }
}

/// JSON parsing error handler
pub struct JsonError(pub JsonRejection);

impl From<JsonRejection> for JsonError {
    fn from(rejection: JsonRejection) -> Self {
        Self(rejection)
    }
}

impl IntoResponse for JsonError {
    fn into_response(self) -> Response {
        let message = match &self.0 {
            JsonRejection::JsonDataError(e) => format!("Invalid JSON data: {}", e),
            JsonRejection::JsonSyntaxError(e) => format!("JSON syntax error: {}", e),
            JsonRejection::MissingJsonContentType(_) => {
                "Missing Content-Type: application/json header".to_string()
            }
            _ => "Invalid JSON request".to_string(),
        };

        let body = ErrorResponse {
            error: ErrorBody {
                code: "INVALID_JSON",
                message,
                details: None,
                field_errors: None,
            },
        };

        (StatusCode::BAD_REQUEST, Json(body)).into_response()
    }
}

/// Result type for API handlers
pub type ApiResult<T> = std::result::Result<T, ApiError>;

/// Result type for validated API handlers
pub type ValidatedResult<T> = std::result::Result<T, ValidationError>;
