//! Repository implementations for database operations

mod audit_repo;
mod invoice_repo;
mod metrics_repo;
mod vendor_repo;
mod workflow_repo;

pub use audit_repo::AuditRepositoryImpl;
pub use invoice_repo::InvoiceRepositoryImpl;
pub use metrics_repo::{MetricsRepositoryImpl, InvoiceMetrics, ApprovalMetrics, VendorMetrics, TopVendor, TeamMetrics, TeamMemberStats};
pub use vendor_repo::VendorRepositoryImpl;
pub use workflow_repo::WorkflowRepositoryImpl;

// Type aliases for backward compatibility with API routes
pub type WorkQueueRepositoryImpl = WorkflowRepositoryImpl;
pub type AssignmentRuleRepositoryImpl = WorkflowRepositoryImpl;
