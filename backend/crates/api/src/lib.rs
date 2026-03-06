//! BillForge API Server
//!
//! HTTP API for the BillForge platform.

// Allow unused variables and dead code in stub implementations (TODOs)
#![allow(unused_variables)]
#![allow(dead_code)]

pub mod config;
pub mod error;
pub mod extractors;
pub mod middleware;
pub mod metrics;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod validation;

pub use config::{Config, Environment};
pub use error::{ApiError, ApiResult, ValidationError};
pub use openapi::{ApiDoc, swagger_ui};
pub use state::AppState;
pub use validation::Validator;
