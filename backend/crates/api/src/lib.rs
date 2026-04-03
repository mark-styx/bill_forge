//! BillForge API Server
//!
//! HTTP API for the BillForge platform.

pub mod config;
pub mod error;
pub mod extractors;
pub mod metrics;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod validation;

pub use config::{Config, Environment};
pub use error::{ApiError, ApiResult, ValidationError};
pub use openapi::{swagger_ui, ApiDoc};
pub use state::AppState;
pub use validation::Validator;
