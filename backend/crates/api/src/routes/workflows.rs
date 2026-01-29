//! Workflow routes (Invoice Processing module)

use crate::error::ApiResult;
use crate::extractors::InvoiceProcessingAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use billforge_core::{
    domain::{
        ApprovalStatus, CreateWorkQueueInput, CreateWorkflowRuleInput, WorkQueue, WorkflowRule,
        CreateAssignmentRuleInput, AssignmentRule, QueueItem, BulkOperationInput, BulkOperationResult,
        BulkOperationError, BulkOperationType,
    },
    traits::{InvoiceRepository, WorkflowRuleRepository, WorkQueueRepository, AssignmentRuleRepository},
    types::Pagination,
};
use billforge_email::EmailTemplates;
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        // Workflow rules
        .route("/rules", get(list_rules))
        .route("/rules", post(create_rule))
        .route("/rules/:id", get(get_rule))
        .route("/rules/:id", put(update_rule))
        .route("/rules/:id", delete(delete_rule))
        .route("/rules/:id/activate", post(activate_rule))
        .route("/rules/:id/deactivate", post(deactivate_rule))
        // Work queues
        .route("/queues", get(list_queues))
        .route("/queues", post(create_queue))
        .route("/queues/:id", get(get_queue))
        .route("/queues/:id", put(update_queue))
        .route("/queues/:id", delete(delete_queue))
        .route("/queues/:id/items", get(list_queue_items))
        .route("/queues/:id/items/:item_id/claim", post(claim_item))
        .route("/queues/:id/items/:item_id/complete", post(complete_item))
        // Assignment rules
        .route("/assignment-rules", get(list_assignment_rules))
        .route("/assignment-rules", post(create_assignment_rule))
        .route("/assignment-rules/:id", get(get_assignment_rule))
        .route("/assignment-rules/:id", put(update_assignment_rule))
        .route("/assignment-rules/:id", delete(delete_assignment_rule))
        // Approvals
        .route("/approvals/pending", get(list_pending_approvals))
        .route("/approvals/:id", get(get_approval))
        .route("/approvals/:id/approve", post(approve))
        .route("/approvals/:id/reject", post(reject))
        // Bulk operations
        .route("/bulk", post(bulk_operation))
        // Invoice processing actions
        .route("/invoices/:id/hold", post(put_on_hold))
        .route("/invoices/:id/release", post(release_from_hold))
        .route("/invoices/:id/void", post(void_invoice))
        .route("/invoices/:id/ready-for-payment", post(mark_ready_for_payment))
        .route("/invoices/:id/move-to-queue", post(move_to_queue))
}

#[derive(Debug, Deserialize)]
pub struct ListRulesQuery {
    pub rule_type: Option<String>,
}

async fn list_rules(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Query(query): Query<ListRulesQuery>,
) -> ApiResult<Json<Vec<WorkflowRule>>> {
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(state.db.clone());
    let rules = repo.list(&tenant.tenant_id, None).await?;
    Ok(Json(rules))
}

async fn get_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(state.db.clone());
    let rule = repo.get_by_id(&tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowRule".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(rule))
}

async fn create_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateWorkflowRuleInput>,
) -> ApiResult<Json<WorkflowRule>> {
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(state.db.clone());
    let rule = repo.create(&tenant.tenant_id, input).await?;
    Ok(Json(rule))
}

async fn update_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkflowRuleInput>,
) -> ApiResult<Json<WorkflowRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(state.db.clone());
    let rule = repo.update(&tenant.tenant_id, &rule_id, input).await?;
    Ok(Json(rule))
}

async fn delete_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(state.db.clone());
    repo.delete(&tenant.tenant_id, &rule_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn activate_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(state.db.clone());
    repo.set_active(&tenant.tenant_id, &rule_id, true).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn deactivate_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(state.db.clone());
    repo.set_active(&tenant.tenant_id, &rule_id, false).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Work Queue Handlers
// ============================================================================

async fn list_queues(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<WorkQueue>>> {
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    let queues = repo.list(&tenant.tenant_id).await?;
    Ok(Json(queues))
}

async fn get_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkQueue>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    let queue = repo.get_by_id(&tenant.tenant_id, &queue_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkQueue".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(queue))
}

async fn create_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateWorkQueueInput>,
) -> ApiResult<Json<WorkQueue>> {
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    let queue = repo.create(&tenant.tenant_id, input).await?;
    Ok(Json(queue))
}

async fn update_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkQueueInput>,
) -> ApiResult<Json<WorkQueue>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    let queue = repo.update(&tenant.tenant_id, &queue_id, input).await?;
    Ok(Json(queue))
}

async fn delete_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    repo.delete(&tenant.tenant_id, &queue_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Debug, Deserialize)]
pub struct QueueItemsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

async fn list_queue_items(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Query(query): Query<QueueItemsQuery>,
) -> ApiResult<Json<Vec<QueueItem>>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;
    
    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(50),
    };
    
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    let result = repo.get_items(&tenant.tenant_id, &queue_id, &pagination).await?;
    Ok(Json(result.data))
}

async fn claim_item(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path((queue_id, item_id)): Path<(String, String)>,
) -> ApiResult<Json<QueueItem>> {
    let item_uuid = uuid::Uuid::parse_str(&item_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid item ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    let item = repo.claim_item(&tenant.tenant_id, item_uuid, &user.user_id).await?;
    Ok(Json(item))
}

#[derive(Debug, Deserialize)]
pub struct CompleteItemInput {
    pub action: String,
}

async fn complete_item(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path((queue_id, item_id)): Path<(String, String)>,
    Json(input): Json<CompleteItemInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let item_uuid = uuid::Uuid::parse_str(&item_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid item ID".to_string()))?;
    
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    repo.complete_item(&tenant.tenant_id, item_uuid, &input.action).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Assignment Rule Handlers
// ============================================================================

async fn list_assignment_rules(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<AssignmentRule>>> {
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(state.db.clone());
    let rules = repo.list(&tenant.tenant_id).await?;
    Ok(Json(rules))
}

async fn get_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<AssignmentRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(state.db.clone());
    let rule = repo.get_by_id(&tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "AssignmentRule".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(rule))
}

async fn create_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateAssignmentRuleInput>,
) -> ApiResult<Json<AssignmentRule>> {
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(state.db.clone());
    let rule = repo.create(&tenant.tenant_id, input).await?;
    Ok(Json(rule))
}

async fn update_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateAssignmentRuleInput>,
) -> ApiResult<Json<AssignmentRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(state.db.clone());
    let rule = repo.update(&tenant.tenant_id, &rule_id, input).await?;
    Ok(Json(rule))
}

async fn delete_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(state.db.clone());
    repo.delete(&tenant.tenant_id, &rule_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Approval Handlers
// ============================================================================

/// Response type for pending approvals (more details than domain type)
#[derive(Debug, Serialize)]
pub struct PendingApprovalResponse {
    pub id: String,
    pub invoice_id: String,
    pub status: String,
    pub created_at: String,
    pub invoice_number: Option<String>,
    pub vendor_name: Option<String>,
    pub total_amount: Option<f64>,
}

async fn list_pending_approvals(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<PendingApprovalResponse>>> {
    // For now return empty - full implementation would query approval_requests
    // with status = 'pending' and join with invoices for details
    Ok(Json(vec![]))
}

async fn get_approval(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    // Placeholder - would fetch approval request by ID
    Err(billforge_core::Error::NotFound {
        resource_type: "ApprovalRequest".to_string(),
        id,
    }.into())
}

#[derive(Deserialize)]
pub struct ApprovalInput {
    pub comments: Option<String>,
}

async fn approve(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<ApprovalInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let approval_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval ID".to_string()))?;

    // Get approval request and invoice details
    let db = state.db.tenant(&tenant.tenant_id).await?;
    let conn = db.connection().await;
    let conn_guard = conn.lock().await;

    // Get approval request and related invoice
    let (invoice_id, invoice_number, vendor_name, amount, submitter_email): (String, String, String, i64, Option<String>) = conn_guard
        .query_row(
            r#"SELECT
                ar.invoice_id,
                i.invoice_number,
                COALESCE(i.vendor_name, 'Unknown'),
                COALESCE(i.total_amount, 0),
                (SELECT email FROM users WHERE id = i.created_by LIMIT 1)
            FROM approval_requests ar
            JOIN invoices i ON ar.invoice_id = i.id
            WHERE ar.id = ?"#,
            [approval_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|e| billforge_core::Error::Database(format!("Approval request not found: {}", e)))?;

    // Update approval request status
    conn_guard.execute(
        "UPDATE approval_requests SET status = 'approved', responded_by = ?, responded_at = datetime('now'), comments = ? WHERE id = ?",
        rusqlite::params![user.user_id.0.to_string(), input.comments, approval_id.to_string()],
    ).map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update invoice processing status
    let invoice_uuid = invoice_id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Database("Invalid invoice ID".to_string()))?;
    let invoice_id_typed = billforge_core::InvoiceId(invoice_uuid);

    drop(conn_guard);

    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    invoice_repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id_typed,
        billforge_core::domain::ProcessingStatus::Approved,
    ).await?;

    // Send email notification to submitter
    if let Some(submitter_email) = submitter_email {
        let approver_name = user.email.clone(); // Use email as name for now
        let amount_formatted = format!("${:.2}", amount as f64 / 100.0);

        let (html, text) = EmailTemplates::invoice_approved(
            &invoice_number,
            &vendor_name,
            &amount_formatted,
            &approver_name,
        );

        // Send email in background (don't block the response)
        let email_service = state.email.clone();
        let subject = format!("Invoice {} Approved", invoice_number);
        tokio::spawn(async move {
            if let Err(e) = email_service.send(&submitter_email, &subject, &html, &text).await {
                tracing::error!("Failed to send approval notification email: {}", e);
            }
        });
    }

    Ok(Json(serde_json::json!({
        "message": "Approved",
        "approval_id": id,
        "invoice_id": invoice_id,
        "approved_by": user.user_id.0.to_string()
    })))
}

async fn reject(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<ApprovalInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let approval_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval ID".to_string()))?;

    let reason = input.comments.clone().unwrap_or_else(|| "No reason provided".to_string());

    // Get approval request and invoice details
    let db = state.db.tenant(&tenant.tenant_id).await?;
    let conn = db.connection().await;
    let conn_guard = conn.lock().await;

    // Get approval request and related invoice
    let (invoice_id, invoice_number, vendor_name, amount, submitter_email): (String, String, String, i64, Option<String>) = conn_guard
        .query_row(
            r#"SELECT
                ar.invoice_id,
                i.invoice_number,
                COALESCE(i.vendor_name, 'Unknown'),
                COALESCE(i.total_amount, 0),
                (SELECT email FROM users WHERE id = i.created_by LIMIT 1)
            FROM approval_requests ar
            JOIN invoices i ON ar.invoice_id = i.id
            WHERE ar.id = ?"#,
            [approval_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|e| billforge_core::Error::Database(format!("Approval request not found: {}", e)))?;

    // Update approval request status
    conn_guard.execute(
        "UPDATE approval_requests SET status = 'rejected', responded_by = ?, responded_at = datetime('now'), comments = ? WHERE id = ?",
        rusqlite::params![user.user_id.0.to_string(), &reason, approval_id.to_string()],
    ).map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update invoice processing status
    let invoice_uuid = invoice_id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Database("Invalid invoice ID".to_string()))?;
    let invoice_id_typed = billforge_core::InvoiceId(invoice_uuid);

    drop(conn_guard);

    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    invoice_repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id_typed,
        billforge_core::domain::ProcessingStatus::Rejected,
    ).await?;

    // Send email notification to submitter
    if let Some(submitter_email) = submitter_email {
        let rejecter_name = user.email.clone();
        let amount_formatted = format!("${:.2}", amount as f64 / 100.0);

        let (html, text) = EmailTemplates::invoice_rejected(
            &invoice_number,
            &vendor_name,
            &amount_formatted,
            &rejecter_name,
            &reason,
        );

        // Send email in background
        let email_service = state.email.clone();
        let subject = format!("Invoice {} Rejected", invoice_number);
        tokio::spawn(async move {
            if let Err(e) = email_service.send(&submitter_email, &subject, &html, &text).await {
                tracing::error!("Failed to send rejection notification email: {}", e);
            }
        });
    }

    Ok(Json(serde_json::json!({
        "message": "Rejected",
        "approval_id": id,
        "invoice_id": invoice_id,
        "rejected_by": user.user_id.0.to_string(),
        "reason": reason
    })))
}

// ============================================================================
// Bulk Operations Handler
// ============================================================================

async fn bulk_operation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<BulkOperationInput>,
) -> ApiResult<Json<BulkOperationResult>> {
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    let mut successful = 0;
    let mut errors = Vec::new();

    for invoice_id in &input.invoice_ids {
        let result = match input.operation {
            BulkOperationType::SubmitForPayment => {
                invoice_repo.update_processing_status(
                    &tenant.tenant_id,
                    invoice_id,
                    billforge_core::domain::ProcessingStatus::ReadyForPayment,
                ).await
            }
            BulkOperationType::Approve => {
                invoice_repo.update_processing_status(
                    &tenant.tenant_id,
                    invoice_id,
                    billforge_core::domain::ProcessingStatus::Approved,
                ).await
            }
            BulkOperationType::Reject => {
                invoice_repo.update_processing_status(
                    &tenant.tenant_id,
                    invoice_id,
                    billforge_core::domain::ProcessingStatus::Rejected,
                ).await
            }
            BulkOperationType::MoveToQueue | BulkOperationType::AssignTo => {
                // These require additional parameters - skip for now
                Ok(())
            }
        };

        match result {
            Ok(_) => successful += 1,
            Err(e) => errors.push(BulkOperationError {
                invoice_id: invoice_id.clone(),
                error: e.to_string(),
            }),
        }
    }

    Ok(Json(BulkOperationResult {
        total: input.invoice_ids.len(),
        successful,
        failed: errors.len(),
        errors,
    }))
}

#[derive(Deserialize)]
pub struct HoldInput {
    pub reason: String,
}

async fn put_on_hold(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<HoldInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::OnHold,
    ).await?;

    Ok(Json(serde_json::json!({ "message": "Invoice placed on hold" })))
}

async fn release_from_hold(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::Submitted,
    ).await?;

    Ok(Json(serde_json::json!({ "message": "Invoice released from hold" })))
}

async fn void_invoice(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::Voided,
    ).await?;

    Ok(Json(serde_json::json!({ "message": "Invoice voided" })))
}

async fn mark_ready_for_payment(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(state.db.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::ReadyForPayment,
    ).await?;

    Ok(Json(serde_json::json!({ "message": "Invoice marked ready for payment" })))
}

#[derive(Debug, Deserialize)]
pub struct MoveToQueueInput {
    pub queue_id: String,
    pub assign_to: Option<String>,
}

async fn move_to_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<MoveToQueueInput>,
) -> ApiResult<Json<QueueItem>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    let queue_id = input.queue_id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;
    
    let assign_to = if let Some(ref user_id_str) = input.assign_to {
        Some(billforge_core::UserId(
            uuid::Uuid::parse_str(user_id_str)
                .map_err(|_| billforge_core::Error::Validation("Invalid user ID".to_string()))?
        ))
    } else {
        None
    };
    
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(state.db.clone());
    let item = repo.move_item(&tenant.tenant_id, &invoice_id, &queue_id, assign_to.as_ref()).await?;
    
    Ok(Json(item))
}
