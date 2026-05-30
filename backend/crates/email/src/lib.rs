//! BillForge Email Service
//!
//! Provides email notification functionality for the platform.

mod inbound;
mod service;
mod statement_ingest;
mod templates;

pub use inbound::{
    extract_domain, extract_email, is_usable_attachment, InboundAttachment, InboundEmailHandler,
    InboundEmailPayload, InboundEmailResult,
};
pub use service::{EmailConfig, EmailService, EmailServiceImpl, MockEmailService};
pub use statement_ingest::{
    classify, extract_attachment_text, ingest_statement, parse_statement_lines, InboundKind,
    ParsedStatementLine,
};
pub use templates::*;
