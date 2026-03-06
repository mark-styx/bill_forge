//! BillForge Core Library
//!
//! This crate contains shared domain types, traits, and utilities used across
//! all BillForge modules. It provides the foundational building blocks for
//! multi-tenant data isolation and module interoperability.

pub mod domain;
pub mod error;
pub mod personas;
pub mod services;
pub mod traits;
pub mod types;
pub mod workflow_service;

#[cfg(test)]
mod tests;

pub use domain::*;
pub use error::{Error, Result};
pub use personas::*;
pub use services::*;
pub use types::*;
pub use workflow_service::*;
