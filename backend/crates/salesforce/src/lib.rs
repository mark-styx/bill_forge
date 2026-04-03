//! Salesforce CRM integration service
//!
//! Provides OAuth 2.0 authentication and REST API client for Salesforce:
//! - OAuth 2.0 flow (Web Server flow)
//! - Account sync (Salesforce Accounts → BillForge Vendors)
//! - Contact sync (Salesforce Contacts → BillForge vendor contacts)
//! - Purchase Order / Opportunity linkage
//! - Custom object support for AP workflow metadata
//!
//! Salesforce integration enriches the AP workflow by:
//! - Auto-matching invoices to Salesforce Accounts (vendor master sync)
//! - Linking PO numbers from Opportunities or custom PO objects
//! - Pushing payment status back to Salesforce for vendor relationship tracking

#![allow(unused_variables)]
#![allow(dead_code)]

pub mod client;
pub mod oauth;
pub mod types;

pub use client::SalesforceClient;
pub use oauth::{SalesforceEnvironment, SalesforceOAuth, SalesforceOAuthConfig};
pub use types::*;
