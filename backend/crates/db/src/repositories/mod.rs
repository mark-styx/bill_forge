//! Repository implementations for database operations

mod audit_repo;
mod invoice_repo;
mod vendor_repo;
mod workflow_repo;

pub use audit_repo::AuditRepositoryImpl;
pub use invoice_repo::InvoiceRepositoryImpl;
pub use vendor_repo::VendorRepositoryImpl;
pub use workflow_repo::WorkflowRepositoryImpl;

// Type aliases for backward compatibility with API routes
pub type WorkQueueRepositoryImpl = WorkflowRepositoryImpl;
pub type AssignmentRuleRepositoryImpl = WorkflowRepositoryImpl;
