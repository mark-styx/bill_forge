//! BillForge Email Service
//!
//! Provides email notification functionality for the platform.

mod inbound;
mod service;
mod templates;

pub use inbound::{
    extract_domain, extract_email, is_usable_attachment, InboundAttachment, InboundEmailHandler,
    InboundEmailPayload, InboundEmailResult,
};
pub use service::{EmailConfig, EmailService, EmailServiceImpl, MockEmailService};
pub use templates::*;
