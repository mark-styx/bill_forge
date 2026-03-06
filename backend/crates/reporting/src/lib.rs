//! Reporting Module
//!
//! Analytics and reporting for BillForge platform.

// Allow dead code and unused variables in stub implementations
#![allow(dead_code)]
#![allow(unused_variables)]

pub mod service;
pub mod models;

pub use service::ReportingService;
pub use models::{DashboardSummary, InvoiceVolume, VendorSpend, AgingBucket, ProcessingMetrics, StatusDistribution, CustomReportQuery, CustomReportResult};
