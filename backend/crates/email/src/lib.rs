//! BillForge Email Service
//!
//! Provides email notification functionality for the platform.

mod service;
mod templates;

pub use service::{EmailConfig, EmailService, EmailServiceImpl, MockEmailService};
pub use templates::*;
