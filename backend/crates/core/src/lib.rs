//! BillForge Core Library
//!
//! This crate contains shared domain types, traits, and utilities used across
//! all BillForge modules. It provides the foundational building blocks for
//! multi-tenant data isolation and module interoperability.

pub mod domain;
pub mod error;
pub mod intelligent_routing;
pub mod personas;
pub mod services;
pub mod traits;
pub mod types;
pub mod workflow_evaluator;
pub mod workflow_service;
pub mod workload_balancer;
pub mod approver_availability;
pub mod webhook;

#[cfg(test)]
mod tests;

pub use domain::*;
pub use error::{Error, Result};
pub use intelligent_routing::*;
pub use personas::*;
pub use services::*;
pub use traits::*;
pub use types::*;
pub use workflow_evaluator::*;
pub use workflow_service::*;
pub use workload_balancer::*;
pub use approver_availability::*;
