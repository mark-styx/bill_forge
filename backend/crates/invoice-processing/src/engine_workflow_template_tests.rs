//! Tests for per-invoice workflow template stage cursor (refs #429).
//!
//! Validates that multi-stage workflow templates resume at the next
//! unprocessed stage on re-entry instead of re-evaluating prior stages from
//! order 0. Both tests require a migrated PostgreSQL with the
//! `workflow_stage_progress` and `approval_requests` tables, so they are
//! marked `#[ignore]` for the default unit-test run and exercised in the
//! integration suite.

#![cfg(test)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use billforge_core::{
    domain::{
        ApprovalRequest, ApprovalStatus, CaptureStatus, ConditionField, ConditionOperator,
        CreateInvoiceInput, CreateWorkflowRuleInput, CreateWorkflowTemplateInput, Invoice,
        InvoiceFilters, InvoiceId, ProcessingStatus, RuleCondition, StageType, WorkflowRule,
        WorkflowRuleId, WorkflowRuleType, WorkflowTemplate, WorkflowTemplateId,
        WorkflowTemplateStage,
    },
    traits::{
        ApprovalRepository, InvoiceRepository, WorkflowRuleRepository, WorkflowTemplateRepository,
    },
    types::{Money, PaginatedResponse, Pagination, TenantId, UserId},
    Result,
};
use sqlx::PgPool;

use crate::engine::WorkflowEngine;

// ---------------------------------------------------------------------------
// Stub repositories (mirror the patterns in engine.rs::tests)
// ---------------------------------------------------------------------------

struct StubInvoiceRepo;

#[async_trait]
impl InvoiceRepository for StubInvoiceRepo {
    async fn create(
        &self,
        _tid: &TenantId,
        _input: CreateInvoiceInput,
        _uid: Option<&UserId>,
    ) -> Result<Invoice> {
        unimplemented!()
    }
    async fn get_by_id(&self, _tid: &TenantId, _id: &InvoiceId) -> Result<Option<Invoice>> {
        Ok(None)
    }
    async fn list(
        &self,
        _tid: &TenantId,
        _f: &InvoiceFilters,
        _p: &Pagination,
    ) -> Result<PaginatedResponse<Invoice>> {
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

struct RecordingApprovalRepo {
    created: Mutex<Vec<ApprovalRequest>>,
}

impl RecordingApprovalRepo {
    fn new() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
        }
    }
    fn requests(&self) -> Vec<ApprovalRequest> {
        self.created.lock().unwrap().clone()
    }
}

#[async_trait]
impl ApprovalRepository for RecordingApprovalRepo {
    async fn create(&self, _tid: &TenantId, request: ApprovalRequest) -> Result<ApprovalRequest> {
        self.created.lock().unwrap().push(request.clone());
        Ok(request)
    }
    async fn get_by_id(&self, _tid: &TenantId, _id: uuid::Uuid) -> Result<Option<ApprovalRequest>> {
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
        _id: uuid::Uuid,
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

struct StubRuleRepo;

#[async_trait]
impl WorkflowRuleRepository for StubRuleRepo {
    async fn create(
        &self,
        _tid: &TenantId,
        _input: CreateWorkflowRuleInput,
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
        _input: CreateWorkflowRuleInput,
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
        _rule_type: WorkflowRuleType,
    ) -> Result<Vec<WorkflowRule>> {
        Ok(vec![])
    }
}

struct StubTemplateRepo {
    default: Option<WorkflowTemplate>,
}

#[async_trait]
impl WorkflowTemplateRepository for StubTemplateRepo {
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
        Ok(self.default.clone())
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn approval_stage(order: i32, name: &str, skip: Vec<RuleCondition>) -> WorkflowTemplateStage {
    WorkflowTemplateStage {
        order,
        name: name.to_string(),
        stage_type: StageType::Approval,
        queue_id: None,
        sla_hours: None,
        escalation_hours: None,
        requires_action: true,
        skip_conditions: skip,
        auto_advance_conditions: vec![],
    }
}

fn payment_stage(order: i32, name: &str) -> WorkflowTemplateStage {
    WorkflowTemplateStage {
        order,
        name: name.to_string(),
        stage_type: StageType::Payment,
        queue_id: None,
        sla_hours: None,
        escalation_hours: None,
        requires_action: false,
        skip_conditions: vec![],
        auto_advance_conditions: vec![],
    }
}

fn template_with_stages(
    tenant_id: &TenantId,
    stages: Vec<WorkflowTemplateStage>,
) -> WorkflowTemplate {
    WorkflowTemplate {
        id: WorkflowTemplateId::new(),
        tenant_id: tenant_id.clone(),
        name: "multi-stage".to_string(),
        description: None,
        is_active: true,
        is_default: true,
        stages,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn test_invoice(tenant_id: &TenantId, amount_cents: i64) -> Invoice {
    Invoice {
        id: InvoiceId::new(),
        tenant_id: tenant_id.clone(),
        vendor_id: Some(uuid::Uuid::new_v4()),
        vendor_name: "Cursor Test Vendor".to_string(),
        invoice_number: "INV-CURSOR-001".to_string(),
        invoice_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
        due_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
        po_number: None,
        subtotal: Some(Money {
            amount: amount_cents,
            currency: "USD".to_string(),
        }),
        tax_amount: Some(Money {
            amount: 0,
            currency: "USD".to_string(),
        }),
        total_amount: Money {
            amount: amount_cents,
            currency: "USD".to_string(),
        },
        currency: "USD".to_string(),
        line_items: vec![],
        capture_status: CaptureStatus::Reviewed,
        processing_status: ProcessingStatus::Draft,
        current_queue_id: None,
        assigned_to: None,
        document_id: uuid::Uuid::new_v4(),
        supporting_documents: vec![],
        ocr_confidence: None,
        categorization_confidence: None,
        department: None,
        gl_code: None,
        cost_center: None,
        notes: None,
        tags: vec![],
        custom_fields: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        created_by: Some(UserId(uuid::Uuid::new_v4())),
    }
}

fn build_engine_with_pool(
    pool: PgPool,
    template: WorkflowTemplate,
) -> (WorkflowEngine, Arc<RecordingApprovalRepo>) {
    let approval_repo = Arc::new(RecordingApprovalRepo::new());
    let engine = WorkflowEngine::new(
        Arc::new(StubInvoiceRepo),
        Arc::new(StubRuleRepo),
        approval_repo.clone(),
        Arc::new(StubTemplateRepo {
            default: Some(template),
        }),
    )
    .with_pool(Arc::new(pool));
    (engine, approval_repo)
}

async fn fetch_cursor(
    pool: &PgPool,
    tenant_id: &TenantId,
    invoice_id: &InvoiceId,
    template_id: &WorkflowTemplateId,
) -> Option<(i32, Option<i32>, Option<String>)> {
    sqlx::query_as::<_, (i32, Option<i32>, Option<String>)>(
        r#"SELECT current_stage_order, last_captured_stage_order, last_captured_stage_name
           FROM workflow_stage_progress
           WHERE tenant_id = $1 AND invoice_id = $2 AND template_id = $3"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(invoice_id.0)
    .bind(template_id.0)
    .fetch_optional(pool)
    .await
    .expect("cursor row lookup should succeed")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with workflow_stage_progress + approval_requests tables"]
async fn test_template_resumes_from_last_captured_stage(pool: PgPool) -> sqlx::Result<()> {
    // Three stages: dept_approval (0) -> finance_approval (1) -> payment (2).
    // First call captures dept_approval (PendingApproval). Second call must
    // resume past dept_approval and capture finance_approval. Third call
    // captures payment (ReadyForPayment).
    let tenant_id = TenantId::new();
    let template = template_with_stages(
        &tenant_id,
        vec![
            approval_stage(0, "dept_approval", vec![]),
            approval_stage(1, "finance_approval", vec![]),
            payment_stage(2, "payment"),
        ],
    );
    let template_id = template.id.clone();
    let invoice = test_invoice(&tenant_id, 50_000);

    let (engine, _approvals) = build_engine_with_pool(pool.clone(), template);

    // ---- call 1: dept_approval captures ----
    let outcome = engine
        .process_invoice(&tenant_id, &invoice)
        .await
        .expect("first call should succeed");
    assert_eq!(
        outcome.status,
        ProcessingStatus::PendingApproval,
        "call 1 should capture dept_approval"
    );

    let cursor = fetch_cursor(&pool, &tenant_id, &invoice.id, &template_id)
        .await
        .expect("cursor row should exist after first capture");
    assert_eq!(cursor.0, 0, "current_stage_order pinned to dept_approval");
    assert_eq!(cursor.1, Some(0), "last_captured_stage_order = 0");
    assert_eq!(cursor.2.as_deref(), Some("dept_approval"));

    // ---- call 2: finance_approval captures ----
    let outcome = engine
        .process_invoice(&tenant_id, &invoice)
        .await
        .expect("second call should succeed");
    assert_eq!(
        outcome.status,
        ProcessingStatus::PendingApproval,
        "call 2 should resume past dept_approval and capture finance_approval"
    );

    let cursor = fetch_cursor(&pool, &tenant_id, &invoice.id, &template_id)
        .await
        .expect("cursor row should exist after second capture");
    assert_eq!(cursor.0, 1, "cursor advances to finance_approval");
    assert_eq!(cursor.1, Some(1), "last_captured_stage_order = 1");
    assert_eq!(cursor.2.as_deref(), Some("finance_approval"));

    // ---- call 3: payment captures ----
    let outcome = engine
        .process_invoice(&tenant_id, &invoice)
        .await
        .expect("third call should succeed");
    assert_eq!(
        outcome.status,
        ProcessingStatus::ReadyForPayment,
        "call 3 should resume past finance_approval and capture payment"
    );

    let cursor = fetch_cursor(&pool, &tenant_id, &invoice.id, &template_id)
        .await
        .expect("cursor row should exist after third capture");
    assert_eq!(cursor.0, 2, "cursor advances to payment");
    assert_eq!(cursor.1, Some(2), "last_captured_stage_order = 2");

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with workflow_stage_progress + approval_requests tables"]
async fn test_template_skip_advances_cursor_persistently(pool: PgPool) -> sqlx::Result<()> {
    // Two stages. Stage 0 has a skip_condition (amount > 0) that matches the
    // invoice. First call must skip stage 0, capture stage 1, and persist a
    // cursor that records stage 0 as already-passed (current_stage_order >= 1).
    // On re-entry, even if the skip condition would no longer match (we mutate
    // the invoice to amount=0 before the second call), stage 0 must NOT be
    // re-evaluated. The cursor advance guarantees the second call resumes at
    // stage 1+ -- collapsing prior approval chains is exactly the #429 bug.
    let tenant_id = TenantId::new();
    let skip_when_positive = vec![RuleCondition {
        field: ConditionField::Amount,
        operator: ConditionOperator::GreaterThan,
        value: serde_json::json!(0),
    }];
    let template = template_with_stages(
        &tenant_id,
        vec![
            approval_stage(0, "dept_approval", skip_when_positive),
            approval_stage(1, "finance_approval", vec![]),
        ],
    );
    let template_id = template.id.clone();
    let mut invoice = test_invoice(&tenant_id, 50_000);

    let (engine, approval_repo) = build_engine_with_pool(pool.clone(), template);

    // ---- call 1: skip dept_approval (amount > 0), capture finance_approval ----
    let outcome = engine
        .process_invoice(&tenant_id, &invoice)
        .await
        .expect("first call should succeed");
    assert_eq!(
        outcome.status,
        ProcessingStatus::PendingApproval,
        "call 1 should skip dept_approval and capture finance_approval"
    );
    assert_eq!(
        approval_repo.requests().len(),
        1,
        "exactly one approval request from finance_approval"
    );

    let cursor = fetch_cursor(&pool, &tenant_id, &invoice.id, &template_id)
        .await
        .expect("cursor row should exist after first capture");
    assert!(
        cursor.0 >= 1,
        "cursor current_stage_order must be at finance_approval or beyond, was {}",
        cursor.0
    );
    assert_eq!(cursor.1, Some(1), "last_captured_stage_order = 1");
    assert_eq!(cursor.2.as_deref(), Some("finance_approval"));

    // ---- mutate invoice so dept_approval's skip would NO LONGER match ----
    invoice.total_amount = Money {
        amount: 0,
        currency: "USD".to_string(),
    };
    invoice.subtotal = Some(Money {
        amount: 0,
        currency: "USD".to_string(),
    });

    // ---- call 2: dept_approval MUST NOT be re-evaluated; cursor resumes past it ----
    let outcome = engine
        .process_invoice(&tenant_id, &invoice)
        .await
        .expect("second call should succeed");
    // Past last_captured: no stage left after finance_approval, so the
    // template returns None and falls through to rule processing. With no
    // rules configured, the rule path approves by default. The critical
    // assertion is that we do NOT see a second approval request from
    // dept_approval (the bug would have re-routed it).
    assert!(
        matches!(
            outcome.status,
            ProcessingStatus::Approved | ProcessingStatus::PendingApproval
        ),
        "outcome status was {:?}",
        outcome.status
    );
    assert_eq!(
        approval_repo.requests().len(),
        1,
        "dept_approval must not be re-evaluated on resume (no new request)"
    );

    Ok(())
}
