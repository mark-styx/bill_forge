//! BillForge API Server
//!
//! HTTP API for the BillForge platform.

pub mod config;
pub mod error;
pub mod extractors;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod validation;

pub use config::{Config, Environment};
pub use error::{ApiError, ApiResult, ValidationError};
pub use openapi::{ApiDoc, swagger_ui};
pub use state::AppState;
pub use validation::Validator;
