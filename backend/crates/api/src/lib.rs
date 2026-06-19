//! BillForge API Server
//!
//! HTTP API for the BillForge platform.

#![allow(warnings)]

pub mod config;
pub mod error;
pub mod extractors;
pub mod fraud_guard;
#[cfg(feature = "capture")]
pub mod invoice_capture;
pub mod metrics;
pub mod middleware;
pub mod ofac_screening;
pub mod openapi;
pub mod routes;
pub mod services;
pub mod starter_packs;
pub mod state;
pub mod state_machine;
pub mod teams_jwt;
pub mod validation;

// Re-export close period lock check for use by other handlers
pub use routes::close_periods::find_locked_period_for_date;

pub use config::{Config, Environment};
pub use error::{ApiError, ApiResult, ValidationError};
pub use openapi::{swagger_ui, ApiDoc};
pub use routes::vendors::{get_routing_rules, RoutingRules, UpdateVendorRequest};
pub use state::AppState;
pub use validation::Validator;
