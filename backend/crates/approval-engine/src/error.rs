//! Approval engine error types

use thiserror::Error;

/// Errors that can occur during approval engine operations
#[derive(Error, Debug)]
pub enum ApprovalError {
    #[error("Policy not found: {0}")]
    PolicyNotFound(String),

    #[error("Chain not found: {0}")]
    ChainNotFound(String),

    #[error("Step not found: {0}")]
    StepNotFound(String),

    #[error("Not authorized to approve: {0}")]
    NotAuthorized(String),

    #[error("Invalid state transition: {current} -> {requested}")]
    InvalidTransition { current: String, requested: String },

    #[error("Chain already completed")]
    AlreadyCompleted,

    #[error("Self-approval not allowed")]
    SelfApprovalNotAllowed,

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<ApprovalError> for billforge_core::Error {
    fn from(e: ApprovalError) -> Self {
        match e {
            ApprovalError::PolicyNotFound(id) => billforge_core::Error::NotFound {
                resource_type: "ApprovalPolicy".to_string(),
                id,
            },
            ApprovalError::ChainNotFound(id) => billforge_core::Error::NotFound {
                resource_type: "ApprovalChain".to_string(),
                id,
            },
            ApprovalError::StepNotFound(id) => billforge_core::Error::NotFound {
                resource_type: "ApprovalStep".to_string(),
                id,
            },
            ApprovalError::NotAuthorized(msg) => billforge_core::Error::Forbidden(msg),
            ApprovalError::InvalidTransition { current, requested } => {
                billforge_core::Error::InvalidStateTransition {
                    from: current,
                    to: requested,
                }
            }
            ApprovalError::AlreadyCompleted => {
                billforge_core::Error::Conflict("Approval chain already completed".to_string())
            }
            ApprovalError::SelfApprovalNotAllowed => {
                billforge_core::Error::Forbidden("Self-approval not allowed".to_string())
            }
            ApprovalError::Database(e) => billforge_core::Error::Database(e.to_string()),
            ApprovalError::Internal(msg) => billforge_core::Error::Internal(msg),
        }
    }
}
