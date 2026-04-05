//! Bill.com (BILL) AP payments integration service
//!
//! Provides session-based authentication and REST API client for Bill.com:
//! - Session-based authentication (devKey + orgId + credentials)
//! - Vendor sync (Bill.com Vendors ↔ BillForge Vendors)
//! - Bill creation and sync (push approved invoices to Bill.com)
//! - Payment execution (ACH, check, virtual card)
//! - Bulk payment support
//! - Payment status tracking

#![allow(unused_variables)]
#![allow(dead_code)]

pub mod auth;
pub mod client;
pub mod types;

pub use auth::{BillComAuth, BillComAuthConfig, BillComEnvironment};
pub use client::{BillComClient, ClientError};
pub use types::*;
