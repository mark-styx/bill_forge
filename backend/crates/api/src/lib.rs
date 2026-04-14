//! BillForge API Server
//!
//! HTTP API for the BillForge platform.

pub mod config;
pub mod error;
pub mod extractors;
pub mod invoice_capture;
pub mod middleware;
pub mod metrics;
pub mod openapi;
pub mod routes;
pub mod state;
pub mod state_machine;
pub mod validation;

pub use config::{Config, Environment};
pub use error::{ApiError, ApiResult, ValidationError};
pub use openapi::{ApiDoc, swagger_ui};
pub use routes::vendors::{RoutingRules, UpdateVendorRequest, get_routing_rules};
pub use state::AppState;
pub use validation::Validator;
