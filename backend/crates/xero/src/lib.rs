//! Xero accounting integration service
//!
//! Provides OAuth 2.0 authentication and API client for Xero:
//! - OAuth 2.0 flow
//! - Contact sync (Xero → BillForge vendors)
//! - Invoice export (BillForge → Xero bills)
//! - Account/Category mapping

// Allow non-snake_case field names to match Xero API conventions
#![allow(non_snake_case)]
#![allow(unused_variables)]
#![allow(dead_code)]

pub mod client;
pub mod oauth;
pub mod types;

pub use client::XeroClient;
pub use oauth::{XeroOAuth, XeroOAuthConfig, XeroEnvironment};
pub use types::*;
