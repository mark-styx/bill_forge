//! Workday Financial Management integration service
//!
//! Provides OAuth 2.0 authentication and API client for Workday Financial Management:
//! - OAuth 2.0 authentication (API Client registration flow)
//! - Supplier sync (Workday Suppliers → BillForge Vendors)
//! - Supplier invoice creation (BillForge → Workday)
//! - Ledger account mapping
//! - Spend category sync
//! - Multi-company support

#![allow(unused_variables)]
#![allow(dead_code)]

pub mod auth;
pub mod client;
pub mod types;

pub use auth::{WorkdayOAuth, WorkdayOAuthConfig};
pub use client::WorkdayClient;
pub use types::*;
