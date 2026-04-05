//! Repository implementations for database operations

mod audit_repo;
mod invoice_repo;
mod metrics_repo;
mod purchase_order_repo;
mod status_config_repo;
mod tax_document_repo;
mod user_repo;
mod payment_request_repo;
mod vendor_repo;
mod vendor_statement_repo;
mod workflow_repo;

pub use audit_repo::AuditRepositoryImpl;
pub use invoice_repo::InvoiceRepositoryImpl;
pub use metrics_repo::{MetricsRepositoryImpl, InvoiceMetrics, ApprovalMetrics, VendorMetrics, TopVendor, TeamMetrics, TeamMemberStats};
pub use purchase_order_repo::PurchaseOrderRepositoryImpl;
pub use status_config_repo::InvoiceStatusConfigRepositoryImpl;
pub use tax_document_repo::TaxDocumentRepositoryImpl;
pub use user_repo::UserRepositoryImpl;
pub use vendor_repo::VendorRepositoryImpl;
pub use payment_request_repo::{PaymentRequestRepositoryImpl, PaymentRequest, PaymentRequestItem};
pub use vendor_statement_repo::VendorStatementRepositoryImpl;
pub use workflow_repo::WorkflowRepositoryImpl;

// Type aliases for backward compatibility with API routes
pub type WorkQueueRepositoryImpl = WorkflowRepositoryImpl;
pub type AssignmentRuleRepositoryImpl = WorkflowRepositoryImpl;
