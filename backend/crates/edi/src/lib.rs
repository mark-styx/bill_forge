//! EDI (Electronic Data Interchange) integration for BillForge
//!
//! Integrates with API-based EDI middleware (Stedi, Orderful, SPS Commerce)
//! to receive and send EDI documents as normalized JSON.
//!
//! Supported document types:
//! - X12 810 (Invoice) - inbound, maps to BillForge invoices
//! - X12 997 (Functional Acknowledgment) - inbound/outbound
//!
//! BillForge does NOT parse raw X12. The middleware handles:
//! - X12/EDIFACT parsing and generation
//! - AS2/SFTP transport
//! - Trading partner connectivity
//! - Compliance validation

pub mod client;
pub mod config;
pub mod mapper;
pub mod types;
pub mod webhook;

pub use client::EdiClient;
pub use config::EdiConfig;
pub use mapper::EdiMapper;
pub use types::*;
pub use webhook::verify_webhook_signature;
