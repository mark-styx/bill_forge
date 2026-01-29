//! Core traits defining interfaces for repositories and services

use crate::domain::*;
use crate::error::Result;
use crate::types::*;
use async_trait::async_trait;
use uuid::Uuid;

/// Repository for invoice operations
#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, input: CreateInvoiceInput, created_by: &UserId) -> Result<Invoice>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: &InvoiceId) -> Result<Option<Invoice>>;
    async fn list(&self, tenant_id: &TenantId, filters: &InvoiceFilters, pagination: &Pagination) -> Result<PaginatedResponse<Invoice>>;
    async fn update(&self, tenant_id: &TenantId, id: &InvoiceId, updates: serde_json::Value) -> Result<Invoice>;
    async fn delete(&self, tenant_id: &TenantId, id: &InvoiceId) -> Result<()>;
    async fn update_capture_status(&self, tenant_id: &TenantId, id: &InvoiceId, status: CaptureStatus) -> Result<()>;
    async fn update_processing_status(&self, tenant_id: &TenantId, id: &InvoiceId, status: ProcessingStatus) -> Result<()>;
}

/// Repository for vendor operations
#[async_trait]
pub trait VendorRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, input: CreateVendorInput) -> Result<Vendor>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: &VendorId) -> Result<Option<Vendor>>;
    async fn list(&self, tenant_id: &TenantId, filters: &VendorFilters, pagination: &Pagination) -> Result<PaginatedResponse<Vendor>>;
    async fn update(&self, tenant_id: &TenantId, id: &VendorId, input: UpdateVendorInput) -> Result<Vendor>;
    async fn delete(&self, tenant_id: &TenantId, id: &VendorId) -> Result<()>;
    async fn find_by_name(&self, tenant_id: &TenantId, name: &str) -> Result<Option<Vendor>>;
    async fn add_contact(&self, tenant_id: &TenantId, vendor_id: &VendorId, contact: VendorContact) -> Result<()>;
    async fn remove_contact(&self, tenant_id: &TenantId, vendor_id: &VendorId, contact_id: Uuid) -> Result<()>;
}

/// Repository for tax documents
#[async_trait]
pub trait TaxDocumentRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, doc: TaxDocument) -> Result<TaxDocument>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: Uuid) -> Result<Option<TaxDocument>>;
    async fn list_for_vendor(&self, tenant_id: &TenantId, vendor_id: &VendorId) -> Result<Vec<TaxDocument>>;
    async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> Result<()>;
}

/// Repository for workflow rules
#[async_trait]
pub trait WorkflowRuleRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, input: CreateWorkflowRuleInput) -> Result<WorkflowRule>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: &WorkflowRuleId) -> Result<Option<WorkflowRule>>;
    async fn list(&self, tenant_id: &TenantId, rule_type: Option<WorkflowRuleType>) -> Result<Vec<WorkflowRule>>;
    async fn update(&self, tenant_id: &TenantId, id: &WorkflowRuleId, input: CreateWorkflowRuleInput) -> Result<WorkflowRule>;
    async fn delete(&self, tenant_id: &TenantId, id: &WorkflowRuleId) -> Result<()>;
    async fn set_active(&self, tenant_id: &TenantId, id: &WorkflowRuleId, is_active: bool) -> Result<()>;
    async fn get_active_rules(&self, tenant_id: &TenantId, rule_type: WorkflowRuleType) -> Result<Vec<WorkflowRule>>;
}

/// Repository for work queues
#[async_trait]
pub trait WorkQueueRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, input: CreateWorkQueueInput) -> Result<WorkQueue>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: &WorkQueueId) -> Result<Option<WorkQueue>>;
    async fn list(&self, tenant_id: &TenantId) -> Result<Vec<WorkQueue>>;
    async fn update(&self, tenant_id: &TenantId, id: &WorkQueueId, input: CreateWorkQueueInput) -> Result<WorkQueue>;
    async fn delete(&self, tenant_id: &TenantId, id: &WorkQueueId) -> Result<()>;
    async fn get_default(&self, tenant_id: &TenantId) -> Result<Option<WorkQueue>>;
    async fn get_by_type(&self, tenant_id: &TenantId, queue_type: QueueType) -> Result<Option<WorkQueue>>;
    async fn add_item(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, invoice_id: &InvoiceId, assigned_to: Option<&UserId>) -> Result<QueueItem>;
    async fn get_items(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, pagination: &Pagination) -> Result<PaginatedResponse<QueueItem>>;
    async fn get_items_for_user(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, user_id: &UserId, pagination: &Pagination) -> Result<PaginatedResponse<QueueItem>>;
    async fn claim_item(&self, tenant_id: &TenantId, item_id: Uuid, user_id: &UserId) -> Result<QueueItem>;
    async fn complete_item(&self, tenant_id: &TenantId, item_id: Uuid, action: &str) -> Result<()>;
    async fn move_item(&self, tenant_id: &TenantId, invoice_id: &InvoiceId, to_queue_id: &WorkQueueId, assigned_to: Option<&UserId>) -> Result<QueueItem>;
    async fn count_items(&self, tenant_id: &TenantId, queue_id: &WorkQueueId) -> Result<i64>;
    async fn count_items_for_user(&self, tenant_id: &TenantId, queue_id: &WorkQueueId, user_id: &UserId) -> Result<i64>;
}

/// Repository for assignment rules
#[async_trait]
pub trait AssignmentRuleRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, input: CreateAssignmentRuleInput) -> Result<AssignmentRule>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: &AssignmentRuleId) -> Result<Option<AssignmentRule>>;
    async fn list_for_queue(&self, tenant_id: &TenantId, queue_id: &WorkQueueId) -> Result<Vec<AssignmentRule>>;
    async fn list(&self, tenant_id: &TenantId) -> Result<Vec<AssignmentRule>>;
    async fn update(&self, tenant_id: &TenantId, id: &AssignmentRuleId, input: CreateAssignmentRuleInput) -> Result<AssignmentRule>;
    async fn delete(&self, tenant_id: &TenantId, id: &AssignmentRuleId) -> Result<()>;
    async fn set_active(&self, tenant_id: &TenantId, id: &AssignmentRuleId, is_active: bool) -> Result<()>;
}

/// Repository for vendor/department approvers
#[async_trait]
pub trait ApproverRegistrationRepository: Send + Sync {
    async fn register_vendor_approver(&self, tenant_id: &TenantId, vendor_id: &VendorId, user_id: &UserId, max_amount: Option<Money>) -> Result<VendorApproverRegistration>;
    async fn get_vendor_approvers(&self, tenant_id: &TenantId, vendor_id: &VendorId) -> Result<Vec<VendorApproverRegistration>>;
    async fn remove_vendor_approver(&self, tenant_id: &TenantId, vendor_id: &VendorId, user_id: &UserId) -> Result<()>;
    
    async fn register_department_approver(&self, tenant_id: &TenantId, department: &str, user_id: &UserId, max_amount: Option<Money>) -> Result<DepartmentApproverRegistration>;
    async fn get_department_approvers(&self, tenant_id: &TenantId, department: &str) -> Result<Vec<DepartmentApproverRegistration>>;
    async fn remove_department_approver(&self, tenant_id: &TenantId, department: &str, user_id: &UserId) -> Result<()>;
}

/// Repository for documents
#[async_trait]
pub trait DocumentRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, doc: DocumentRef) -> Result<DocumentRef>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: Uuid) -> Result<Option<DocumentRef>>;
    async fn list_for_invoice(&self, tenant_id: &TenantId, invoice_id: &InvoiceId) -> Result<Vec<DocumentRef>>;
    async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> Result<()>;
}

/// Repository for approval requests
#[async_trait]
pub trait ApprovalRepository: Send + Sync {
    async fn create(&self, tenant_id: &TenantId, request: ApprovalRequest) -> Result<ApprovalRequest>;
    async fn get_by_id(&self, tenant_id: &TenantId, id: Uuid) -> Result<Option<ApprovalRequest>>;
    async fn list_for_invoice(&self, tenant_id: &TenantId, invoice_id: &InvoiceId) -> Result<Vec<ApprovalRequest>>;
    async fn list_pending_for_user(&self, tenant_id: &TenantId, user_id: &UserId) -> Result<Vec<ApprovalRequest>>;
    async fn respond(&self, tenant_id: &TenantId, id: Uuid, status: ApprovalStatus, comments: Option<String>, user_id: &UserId) -> Result<ApprovalRequest>;
    async fn cancel_for_invoice(&self, tenant_id: &TenantId, invoice_id: &InvoiceId) -> Result<()>;
}

/// OCR service interface
#[async_trait]
pub trait OcrService: Send + Sync {
    async fn extract(&self, document_bytes: &[u8], mime_type: &str) -> Result<OcrExtractionResult>;
    fn supported_formats(&self) -> Vec<&'static str>;
    fn provider_name(&self) -> &'static str;
}

/// File storage service interface
#[async_trait]
pub trait StorageService: Send + Sync {
    async fn upload(&self, tenant_id: &TenantId, file_name: &str, data: &[u8], mime_type: &str) -> Result<Uuid>;
    async fn download(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<Vec<u8>>;
    async fn delete(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<()>;
    async fn get_url(&self, tenant_id: &TenantId, file_id: Uuid, expires_in_secs: u64) -> Result<String>;
    /// Health check for storage service
    async fn health_check(&self) -> Result<()>;
}

/// Audit log service interface
#[async_trait]
pub trait AuditService: Send + Sync {
    async fn log(&self, entry: AuditEntry) -> Result<()>;
    async fn query(&self, tenant_id: &TenantId, filters: AuditFilters, pagination: &Pagination) -> Result<PaginatedResponse<AuditEntry>>;
}

/// Filters for audit log queries
#[derive(Debug, Clone, Default)]
pub struct AuditFilters {
    pub user_id: Option<UserId>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub from_date: Option<chrono::DateTime<chrono::Utc>>,
    pub to_date: Option<chrono::DateTime<chrono::Utc>>,
}
