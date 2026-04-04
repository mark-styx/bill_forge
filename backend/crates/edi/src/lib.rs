//! EDI (Electronic Data Interchange) integration for BillForge
//!
//! Integrates with API-based EDI middleware (Stedi, Orderful, SPS Commerce)
//! to receive and send EDI documents as normalized JSON.
//!
//! Supported document types:
//! - X12 810 (Invoice) - inbound, maps to BillForge invoices
//! - X12 820 (Payment Remittance) - outbound, sent after invoice payment
//! - X12 850 (Purchase Order) - inbound, creates purchase orders
//! - X12 856 (Advance Ship Notice) - inbound, creates receiving records
//! - X12 997 (Functional Acknowledgment) - inbound/outbound, ack state machine
//!
//! BillForge does NOT parse raw X12. The middleware handles:
//! - X12/EDIFACT parsing and generation
//! - AS2/SFTP transport
//! - Trading partner connectivity
//! - Compliance validation

pub mod client;
pub mod config;
pub mod mapper;
pub mod matching;
pub mod outbound;
pub mod types;
pub mod webhook;

pub use client::EdiClient;
pub use config::EdiConfig;
pub use mapper::EdiMapper;
pub use matching::MatchEngine;
pub use outbound::{OutboundEdiService, check_ack_timeouts, process_inbound_ack};
pub use types::*;
pub use webhook::verify_webhook_signature;
