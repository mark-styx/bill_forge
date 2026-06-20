//! BillForge Email Service
//!
//! Provides email notification functionality for the platform.

pub mod approval_reply;
mod inbound;
mod service;
mod statement_ingest;
mod templates;

pub use approval_reply::{
    extract_action_token, handle_approval_reply, parse_reply_command, ApprovalReplyOutcome,
    ReplyCommand,
};
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
