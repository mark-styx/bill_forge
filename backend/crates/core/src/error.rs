//! Error types for BillForge
//!
//! Provides a unified error handling approach across all modules.

use thiserror::Error;

/// Result type alias using BillForge's Error
pub type Result<T> = std::result::Result<T, Error>;

/// Main error enum for BillForge operations
#[derive(Error, Debug)]
pub enum Error {
    // Authentication & Authorization
    #[error("Authentication required")]
    Unauthenticated,

    #[error("Access denied: {0}")]
    Forbidden(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    // Tenant Isolation
    #[error("Tenant not found: {0}")]
    TenantNotFound(String),

    #[error("Tenant context required")]
    TenantContextMissing,

    #[error("Cross-tenant access denied")]
    CrossTenantAccess,

    // Resource Errors
    #[error("Resource not found: {resource_type} with id {id}")]
    NotFound { resource_type: String, id: String },

    #[error("Resource already exists: {resource_type}")]
    AlreadyExists { resource_type: String },

    #[error("Conflict: {0}")]
    Conflict(String),

    // Validation
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid input: {field} - {message}")]
    InvalidInput { field: String, message: String },

    // Module Availability
    #[error("Module not available: {0}. Please contact sales to enable this feature.")]
    ModuleNotAvailable(String),

    #[error("Feature not enabled: {0}")]
    FeatureNotEnabled(String),

    // OCR Errors
    #[error("OCR processing failed: {0}")]
    OcrFailed(String),

    #[error("Unsupported file format: {0}")]
    UnsupportedFormat(String),

    // Workflow Errors
    #[error("Invalid workflow state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Approval required from: {0}")]
    ApprovalRequired(String),

    // Database Errors
    #[error("Database error: {0}")]
    Database(String),

    #[error("Migration error: {0}")]
    Migration(String),

    // External Service Errors
    #[error("External service error: {service} - {message}")]
    ExternalService { service: String, message: String },

    // Storage Errors
    #[error("File storage error: {0}")]
    Storage(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    // Rate Limiting
    #[error("Rate limit exceeded. Try again in {retry_after} seconds.")]
    RateLimited { retry_after: u64 },

    // Internal Errors
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Configuration error: {0}")]
    Configuration(String),
}

impl Error {
    /// Returns the HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            Error::Unauthenticated | Error::InvalidCredentials | Error::TokenExpired => 401,
            Error::Forbidden(_) | Error::CrossTenantAccess => 403,
            Error::NotFound { .. } | Error::TenantNotFound(_) | Error::FileNotFound(_) => 404,
            Error::AlreadyExists { .. } | Error::Conflict(_) => 409,
            Error::Validation(_) | Error::InvalidInput { .. } | Error::UnsupportedFormat(_) => 400,
            Error::ModuleNotAvailable(_) | Error::FeatureNotEnabled(_) => 402, // Payment Required
            Error::RateLimited { .. } => 429,
            Error::InvalidToken(_) => 401,
            _ => 500,
        }
    }

    /// Returns an error code string for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            Error::Unauthenticated => "UNAUTHENTICATED",
            Error::Forbidden(_) => "FORBIDDEN",
            Error::InvalidCredentials => "INVALID_CREDENTIALS",
            Error::TokenExpired => "TOKEN_EXPIRED",
            Error::InvalidToken(_) => "INVALID_TOKEN",
            Error::TenantNotFound(_) => "TENANT_NOT_FOUND",
            Error::TenantContextMissing => "TENANT_CONTEXT_MISSING",
            Error::CrossTenantAccess => "CROSS_TENANT_ACCESS",
            Error::NotFound { .. } => "NOT_FOUND",
            Error::AlreadyExists { .. } => "ALREADY_EXISTS",
            Error::Conflict(_) => "CONFLICT",
            Error::Validation(_) => "VALIDATION_ERROR",
            Error::InvalidInput { .. } => "INVALID_INPUT",
            Error::ModuleNotAvailable(_) => "MODULE_NOT_AVAILABLE",
            Error::FeatureNotEnabled(_) => "FEATURE_NOT_ENABLED",
            Error::OcrFailed(_) => "OCR_FAILED",
            Error::UnsupportedFormat(_) => "UNSUPPORTED_FORMAT",
            Error::InvalidStateTransition { .. } => "INVALID_STATE_TRANSITION",
            Error::ApprovalRequired(_) => "APPROVAL_REQUIRED",
            Error::Database(_) => "DATABASE_ERROR",
            Error::Migration(_) => "MIGRATION_ERROR",
            Error::ExternalService { .. } => "EXTERNAL_SERVICE_ERROR",
            Error::Storage(_) => "STORAGE_ERROR",
            Error::FileNotFound(_) => "FILE_NOT_FOUND",
            Error::RateLimited { .. } => "RATE_LIMITED",
            Error::Internal(_) => "INTERNAL_ERROR",
            Error::Configuration(_) => "CONFIGURATION_ERROR",
        }
    }
}
