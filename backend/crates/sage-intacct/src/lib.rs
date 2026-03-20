//! Sage Intacct integration service
//!
//! Provides session-based authentication and XML Web Services API client
//! for Sage Intacct ERP:
//! - Session-based authentication (sender credentials + company login)
//! - Vendor sync (Sage Intacct → BillForge)
//! - AP bill export (BillForge → Sage Intacct)
//! - GL account mapping
//! - Multi-entity/subsidiary support

#![allow(unused_variables)]
#![allow(dead_code)]

pub mod auth;
pub mod client;
pub mod types;

pub use auth::{SageIntacctAuth, SageIntacctAuthConfig};
pub use client::SageIntacctClient;
pub use types::*;
