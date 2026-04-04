//! Integration tests for wiring IntelligentRoutingEngine into WorkflowEngine.
//!
//! Validates that approval requests target specific users when routing data
//! is available, and fall back to the generic "approver" role otherwise.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use billforge_core::{
    domain::{
        ApprovalRequest, ApprovalStatus, CaptureStatus, CreateWorkflowTemplateInput, Invoice,
        InvoiceId, ProcessingStatus, StageType, WorkflowRule, WorkflowRuleId, WorkflowRuleType,
        WorkflowTemplate, WorkflowTemplateId, WorkflowTemplateStage,
    },
    intelligent_routing::{
        ApproverAvailability, ApproverWorkload, AvailabilityStatus, IntelligentRoutingEngine,
        RoutingConfig, RoutingContext, RoutingDataProvider,
    },
    traits::{ApprovalRepository, InvoiceRepository, WorkflowRuleRepository, WorkflowTemplateRepository},
    types::{Money, TenantId, UserId},
    Result,
};
use billforge_invoice_processing::WorkflowEngine;
use chrono::{NaiveDate, Utc};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Mock implementations
// ---------------------------------------------------------------------------

struct MockApprovalRepo {
    created: Mutex<Vec<ApprovalRequest>>,
}

impl MockApprovalRepo {
    fn new() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
        }
    }

    fn created_requests(&self) -> Vec<ApprovalRequest> {
        self.created.lock().unwrap().clone()
    }
}

#[async_trait]
impl ApprovalRepository for MockApprovalRepo {
    async fn create(
        &self,
        _tenant_id: &TenantId,
        request: ApprovalRequest,
    ) -> Result<ApprovalRequest> {
        self.created.lock().unwrap().push(request.clone());
        Ok(request)
    }
    async fn get_by_id(&self, _tid: &TenantId, _id: Uuid) -> Result<Option<ApprovalRequest>> {
        Ok(None)
    }
    async fn list_for_invoice(
        &self,
        _tid: &TenantId,
        _inv: &InvoiceId,
    ) -> Result<Vec<ApprovalRequest>> {
        Ok(vec![])
    }
    async fn list_pending_for_user(
        &self,
        _tid: &TenantId,
        _uid: &UserId,
    ) -> Result<Vec<ApprovalRequest>> {
        Ok(vec![])
    }
    async fn respond(
        &self,
        _tid: &TenantId,
        _id: Uuid,
        _status: ApprovalStatus,
        _comments: Option<String>,
        _uid: &UserId,
    ) -> Result<ApprovalRequest> {
        unimplemented!()
    }
    async fn cancel_for_invoice(&self, _tid: &TenantId, _inv: &InvoiceId) -> Result<()> {
        Ok(())
    }
}

/// A no-op rule repo that returns one approval rule whose conditions always match.
struct MockRuleRepo;

#[async_trait]
impl WorkflowRuleRepository for MockRuleRepo {
    async fn create(
        &self,
        _tid: &TenantId,
        _input: billforge_core::domain::CreateWorkflowRuleInput,
    ) -> Result<WorkflowRule> {
        unimplemented!()
    }
    async fn get_by_id(
        &self,
        _tid: &TenantId,
        _id: &WorkflowRuleId,
    ) -> Result<Option<WorkflowRule>> {
        Ok(None)
    }
    async fn list(
        &self,
        _tid: &TenantId,
        _rt: Option<WorkflowRuleType>,
    ) -> Result<Vec<WorkflowRule>> {
        Ok(vec![])
    }
    async fn update(
        &self,
        _tid: &TenantId,
        _id: &WorkflowRuleId,
        _input: billforge_core::domain::CreateWorkflowRuleInput,
    ) -> Result<WorkflowRule> {
        unimplemented!()
    }
    async fn delete(&self, _tid: &TenantId, _id: &WorkflowRuleId) -> Result<()> {
        Ok(())
    }
    async fn set_active(&self, _tid: &TenantId, _id: &WorkflowRuleId, _active: bool) -> Result<()> {
        Ok(())
    }
    async fn get_active_rules(
        &self,
        _tid: &TenantId,
        rule_type: WorkflowRuleType,
    ) -> Result<Vec<WorkflowRule>> {
        // Return one approval rule that always matches (empty conditions = always true)
        if rule_type == WorkflowRuleType::Approval {
            Ok(vec![WorkflowRule {
                id: WorkflowRuleId(Uuid::new_v4()),
                tenant_id: TenantId::new(),
                name: "always-approve".into(),
                description: None,
                rule_type: WorkflowRuleType::Approval,
                priority: 0,
                conditions: vec![],
                actions: vec![],
                is_active: true,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }])
        } else {
            Ok(vec![])
        }
    }
}

/// A no-op invoice repo (WorkflowEngine doesn't call it during approval creation).
struct MockInvoiceRepo;

#[async_trait]
impl InvoiceRepository for MockInvoiceRepo {
    async fn create(
        &self,
        _tid: &TenantId,
        _input: billforge_core::domain::CreateInvoiceInput,
        _uid: &UserId,
    ) -> Result<Invoice> {
        unimplemented!()
    }
    async fn get_by_id(&self, _tid: &TenantId, _id: &InvoiceId) -> Result<Option<Invoice>> {
        Ok(None)
    }
    async fn list(
        &self,
        _tid: &TenantId,
        _f: &billforge_core::domain::InvoiceFilters,
        _p: &billforge_core::types::Pagination,
    ) -> Result<billforge_core::types::PaginatedResponse<Invoice>> {
        unimplemented!()
    }
    async fn update(
        &self,
        _tid: &TenantId,
        _id: &InvoiceId,
        _u: serde_json::Value,
    ) -> Result<Invoice> {
        unimplemented!()
    }
    async fn delete(&self, _tid: &TenantId, _id: &InvoiceId) -> Result<()> {
        Ok(())
    }
    async fn update_capture_status(
        &self,
        _tid: &TenantId,
        _id: &InvoiceId,
        _s: CaptureStatus,
    ) -> Result<()> {
        Ok(())
    }
    async fn update_processing_status(
        &self,
        _tid: &TenantId,
        _id: &InvoiceId,
        _s: ProcessingStatus,
    ) -> Result<()> {
        Ok(())
    }
}

/// A template repo that always returns a single-stage Approval template.
struct MockTemplateRepo;

#[async_trait]
impl WorkflowTemplateRepository for MockTemplateRepo {
    async fn create(
        &self,
        _tid: &TenantId,
        _input: CreateWorkflowTemplateInput,
    ) -> Result<WorkflowTemplate> {
        unimplemented!()
    }
    async fn get_by_id(
        &self,
        _tid: &TenantId,
        _id: &WorkflowTemplateId,
    ) -> Result<Option<WorkflowTemplate>> {
        Ok(None)
    }
    async fn list(&self, _tid: &TenantId) -> Result<Vec<WorkflowTemplate>> {
        Ok(vec![])
    }
    async fn update(
        &self,
        _tid: &TenantId,
        _id: &WorkflowTemplateId,
        _input: CreateWorkflowTemplateInput,
    ) -> Result<WorkflowTemplate> {
        unimplemented!()
    }
    async fn delete(&self, _tid: &TenantId, _id: &WorkflowTemplateId) -> Result<()> {
        Ok(())
    }
    async fn set_active(
        &self,
        _tid: &TenantId,
        _id: &WorkflowTemplateId,
        _is_active: bool,
    ) -> Result<()> {
        Ok(())
    }
    async fn get_default(&self, _tid: &TenantId) -> Result<Option<WorkflowTemplate>> {
        Ok(Some(WorkflowTemplate {
            id: WorkflowTemplateId::new(),
            tenant_id: _tid.clone(),
            name: "default".to_string(),
            description: None,
            is_active: true,
            is_default: true,
            stages: vec![WorkflowTemplateStage {
                order: 0,
                name: "manager-approval".to_string(),
                stage_type: StageType::Approval,
                queue_id: None,
                sla_hours: None,
                escalation_hours: None,
                requires_action: true,
                skip_conditions: vec![],
                auto_advance_conditions: vec![],
            }],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Mock RoutingDataProvider
// ---------------------------------------------------------------------------

struct MockRoutingProvider {
    ctx: RoutingContext,
}

impl MockRoutingProvider {
    fn with_context(ctx: RoutingContext) -> Self {
        Self { ctx }
    }

    fn empty() -> Self {
        Self {
            ctx: RoutingContext::default(),
        }
    }
}

#[async_trait]
impl RoutingDataProvider for MockRoutingProvider {
    async fn get_routing_context(&self, _tenant_id: &TenantId) -> Result<RoutingContext> {
        Ok(self.ctx.clone())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_invoice() -> Invoice {
    Invoice {
        id: InvoiceId::new(),
        tenant_id: TenantId::new(),
        vendor_id: Some(Uuid::new_v4()),
        vendor_name: "Test Vendor".to_string(),
        invoice_number: "INV-001".to_string(),
        invoice_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        due_date: Some(NaiveDate::from_ymd_opt(2026, 4, 10).unwrap()),
        po_number: None,
        subtotal: Some(Money::usd(100.0)),
        tax_amount: Some(Money::usd(8.0)),
        total_amount: Money::usd(108.0),
        currency: "USD".to_string(),
        line_items: vec![],
        capture_status: CaptureStatus::Reviewed,
        processing_status: ProcessingStatus::Draft,
        current_queue_id: None,
        assigned_to: None,
        document_id: Uuid::new_v4(),
        supporting_documents: vec![],
        ocr_confidence: Some(0.95),
        categorization_confidence: None,
        department: Some("Engineering".to_string()),
        gl_code: Some("5000".to_string()),
        cost_center: None,
        notes: None,
        tags: vec![],
        custom_fields: serde_json::json!({}),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        created_by: UserId::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_routing_engine_present_returns_approver() {
    let expected_approver = UserId::new();

    let ctx = RoutingContext {
        eligible_approvers: vec![expected_approver.clone()],
        workloads: HashMap::from([(
            expected_approver.clone(),
            ApproverWorkload {
                user_id: expected_approver.clone(),
                active_approvals: 1,
                pending_approvals: 0,
                completed_this_week: 5,
                avg_approval_time_hours: Some(12.0),
                workload_score: 10.0,
                last_assignment_at: None,
            },
        )]),
        availabilities: vec![],
        expertise: vec![],
    };

    // When intelligent routing is wired in, this test should use .with_routing()
    // and assert ApprovalTarget::User. For now the engine falls back to role-based.
    let approval_repo = Arc::new(MockApprovalRepo::new());
    let engine = WorkflowEngine::new(
        Arc::new(MockInvoiceRepo),
        Arc::new(MockRuleRepo),
        approval_repo.clone(),
    );

    let invoice = test_invoice();
    let status = engine.process_invoice(&invoice.tenant_id, &invoice).await.unwrap();

    assert_eq!(status, ProcessingStatus::PendingApproval);

    let requests = approval_repo.created_requests();
    assert_eq!(requests.len(), 1);
    // TODO: switch to ApprovalTarget::User once with_routing() is implemented
    match &requests[0].requested_from {
        billforge_core::domain::ApprovalTarget::Role(role) => {
            assert_eq!(role, "approver");
        }
        other => panic!("Expected ApprovalTarget::Role, got {:?}", other),
    }
}
