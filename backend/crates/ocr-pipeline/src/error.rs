//! Pipeline-specific error types

use thiserror::Error;

/// Errors that can occur during OCR pipeline operations
#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("OCR extraction failed: {0}")]
    OcrFailed(String),

    #[error("Unsupported file format: {0}")]
    UnsupportedFormat(String),

    #[error("Job not found: {0}")]
    JobNotFound(String),

    #[error("Job already completed: {0}")]
    JobAlreadyCompleted(String),

    #[error("Job cancelled: {0}")]
    JobCancelled(String),

    #[error("Max retries exceeded for job: {0}")]
    MaxRetriesExceeded(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Vendor matching failed: {0}")]
    VendorMatchFailed(String),

    #[error("Batch processing error: {0}")]
    BatchError(String),

    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<PipelineError> for billforge_core::Error {
    fn from(err: PipelineError) -> Self {
        match err {
            PipelineError::OcrFailed(msg) => billforge_core::Error::OcrFailed(msg),
            PipelineError::UnsupportedFormat(msg) => billforge_core::Error::UnsupportedFormat(msg),
            PipelineError::JobNotFound(id) => billforge_core::Error::NotFound {
                resource_type: "OcrJob".to_string(),
                id,
            },
            PipelineError::Storage(msg) => billforge_core::Error::Storage(msg),
            PipelineError::Database(msg) => billforge_core::Error::Database(msg),
            _ => billforge_core::Error::Internal(err.to_string()),
        }
    }
}

impl From<sqlx::Error> for PipelineError {
    fn from(err: sqlx::Error) -> Self {
        PipelineError::Database(err.to_string())
    }
}
