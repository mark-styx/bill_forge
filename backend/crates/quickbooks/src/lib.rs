//! QuickBooks Online integration service
//!
//! Provides OAuth 2.0 authentication and API client for QuickBooks Online:
//! - OAuth 2.0 flow
//! - Vendor sync (QuickBooks → BillForge)
//! - Invoice export (BillForge → QuickBooks)
//! - Account/Category mapping

// Allow non-snake-case field names to match QuickBooks API conventions
#![allow(non_snake_case)]

pub mod client;
pub mod oauth;
pub mod types;

pub use client::{ClientError, QuickBooksClient, UpdateBillRequest, UpdateVendorRequest};
pub use oauth::{QuickBooksEnvironment, QuickBooksOAuth, QuickBooksOAuthConfig};
pub use types::*;
