//! Vendor Management Module
//!
//! Vendor lifecycle management, tax documents, and communication.

pub mod ofac_screening;
pub mod service;

pub use service::VendorService;
