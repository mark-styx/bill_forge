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
        AuditAction, AuditEntry, ResourceType,
        CreateWorkQueueInput, CreateWorkflowRuleInput, WorkQueue, WorkflowRule,
        CreateAssignmentRuleInput, AssignmentRule, QueueItem, BulkOperationInput, BulkOperationResult,
        BulkOperationError, BulkOperationType,
        CreateWorkflowTemplateInput, WorkflowTemplate,
        ApprovalDelegation, CreateApprovalDelegationInput,
        ApprovalLimit, CreateApprovalLimitInput,
        detect_delegation_cycle,
    },
    traits::{AuditService, InvoiceRepository, WorkflowRuleRepository, WorkQueueRepository, AssignmentRuleRepository, WorkflowTemplateRepository, ApprovalDelegationRepository, ApprovalLimitRepository},
    types::Pagination,
};
use billforge_email::EmailTemplates;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persist an audit entry or, if that fails, emit an ERROR-level log with a
/// replayable fingerprint so SOX auditors can spot and reconcile the gap.
async fn log_audit_or_record_gap(pool: &std::sync::Arc<sqlx::PgPool>, entry: AuditEntry) {
    let fingerprint = serde_json::json!({
        "id": entry.id,
        "tenant_id": entry.tenant_id,
        "action": entry.action,
        "resource_type": entry.resource_type,
        "resource_id": entry.resource_id,
        "old_value": entry.old_value,
        "new_value": entry.new_value,
    });
    let repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = repo.log(entry).await {
        tracing::error!(
            error = %e,
            audit_entry = %fingerprint,
            "SOX: failed to persist audit entry — manual reconciliation required"
        );
    }
}

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
        // Workflow templates
        .route("/templates", get(list_templates))
        .route("/templates", post(create_template))
        .route("/templates/:id", get(get_template))
        .route("/templates/:id", put(update_template))
        .route("/templates/:id", delete(delete_template))
        .route("/templates/:id/activate", post(activate_template))
        .route("/templates/:id/deactivate", post(deactivate_template))
        // Approval Delegations
        .route("/delegations", get(list_delegations))
        .route("/delegations", post(create_delegation))
        .route("/delegations/:id", get(get_delegation))
        .route("/delegations/:id", put(update_delegation))
        .route("/delegations/:id", delete(delete_delegation))
        // Approval Limits
        .route("/approval-limits", get(list_approval_limits))
        .route("/approval-limits", post(create_approval_limit))
        .route("/approval-limits/:id", get(get_approval_limit))
        .route("/approval-limits/:id", put(update_approval_limit))
        .route("/approval-limits/:id", delete(delete_approval_limit))
        // Invoice processing actions
        .route("/invoices/:id/hold", post(put_on_hold))
        .route("/invoices/:id/release", post(release_from_hold))
        .route("/invoices/:id/void", post(void_invoice))
        .route("/invoices/:id/ready-for-payment", post(mark_ready_for_payment))
        .route("/invoices/:id/move-to-queue", post(move_to_queue))
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ListRulesQuery {
    pub rule_type: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/workflows/rules", tag = "Workflows", responses((status = 200, description = "Workflow rules")))]
async fn list_rules(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Query(_query): Query<ListRulesQuery>,
) -> ApiResult<Json<Vec<WorkflowRule>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let rules = billforge_core::traits::WorkflowRuleRepository::list(&repo, &tenant.tenant_id, None).await?;
    Ok(Json(rules))
}

#[utoipa::path(get, path = "/api/v1/workflows/rules/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Rule details")))]
async fn get_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let rule = billforge_core::traits::WorkflowRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowRule".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(rule))
}

#[utoipa::path(post, path = "/api/v1/workflows/rules", tag = "Workflows", request_body = serde_json::Value, responses((status = 200, description = "Rule created")))]
async fn create_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateWorkflowRuleInput>,
) -> ApiResult<Json<WorkflowRule>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let rule = WorkflowRuleRepository::create(&repo, &tenant.tenant_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Create, ResourceType::WorkflowRule,
        rule.id.to_string(), format!("Created workflow rule '{}'", rule.name),
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(rule))
}

#[utoipa::path(put, path = "/api/v1/workflows/rules/{id}", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Rule updated")))]
async fn update_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkflowRuleInput>,
) -> ApiResult<Json<WorkflowRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    // Fetch before-state for SOX audit trail
    let old_rule = WorkflowRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowRule".to_string(),
            id: id.clone(),
        })?;

    let rule = WorkflowRuleRepository::update(&repo, &tenant.tenant_id, &rule_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkflowRule,
        id.clone(), format!("Updated workflow rule '{}'", rule.name),
    ).with_user_email(&user.email)
     .with_old_value(serde_json::to_value(&old_rule).unwrap_or(serde_json::Value::Null))
     .with_new_value(serde_json::to_value(&rule).unwrap_or(serde_json::Value::Null));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(rule))
}

#[utoipa::path(delete, path = "/api/v1/workflows/rules/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Rule deleted")))]
async fn delete_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    // Capture before-state for SOX audit trail (what was destroyed)
    let old_rule = WorkflowRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id).await?;
    WorkflowRuleRepository::delete(&repo, &tenant.tenant_id, &rule_id).await?;

    let mut audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Delete, ResourceType::WorkflowRule,
        id.clone(), "Deleted workflow rule",
    ).with_user_email(&user.email);
    if let Some(old) = old_rule {
        audit_entry = audit_entry.with_old_value(serde_json::to_value(&old).unwrap_or(serde_json::Value::Null));
    }
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(post, path = "/api/v1/workflows/rules/{id}/activate", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Rule activated")))]
async fn activate_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    // Fetch before-state for SOX audit trail
    let old_rule = WorkflowRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowRule".to_string(),
            id: id.clone(),
        })?;

    WorkflowRuleRepository::set_active(&repo, &tenant.tenant_id, &rule_id, true).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkflowRule,
        id.clone(), "Activated workflow rule",
    ).with_user_email(&user.email)
     .with_old_value(serde_json::json!({ "is_active": old_rule.is_active }))
     .with_new_value(serde_json::json!({ "is_active": true }));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(post, path = "/api/v1/workflows/rules/{id}/deactivate", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Rule deactivated")))]
async fn deactivate_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    // Fetch before-state for SOX audit trail
    let old_rule = WorkflowRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowRule".to_string(),
            id: id.clone(),
        })?;

    WorkflowRuleRepository::set_active(&repo, &tenant.tenant_id, &rule_id, false).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkflowRule,
        id.clone(), "Deactivated workflow rule",
    ).with_user_email(&user.email)
     .with_old_value(serde_json::json!({ "is_active": old_rule.is_active }))
     .with_new_value(serde_json::json!({ "is_active": false }));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Work Queue Handlers
// ============================================================================

#[utoipa::path(get, path = "/api/v1/workflows/queues", tag = "Workflows", responses((status = 200, description = "Work queues")))]
async fn list_queues(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<WorkQueue>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    let queues = WorkQueueRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(queues))
}

#[utoipa::path(get, path = "/api/v1/workflows/queues/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Queue details")))]
async fn get_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkQueue>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    let queue = WorkQueueRepository::get_by_id(&repo, &tenant.tenant_id, &queue_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkQueue".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(queue))
}

#[utoipa::path(post, path = "/api/v1/workflows/queues", tag = "Workflows", request_body = serde_json::Value, responses((status = 200, description = "Queue created")))]
async fn create_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateWorkQueueInput>,
) -> ApiResult<Json<WorkQueue>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    let queue = WorkQueueRepository::create(&repo, &tenant.tenant_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Create, ResourceType::WorkQueue,
        queue.id.to_string(), format!("Created work queue '{}'", queue.name),
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(queue))
}

#[utoipa::path(put, path = "/api/v1/workflows/queues/{id}", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Queue updated")))]
async fn update_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkQueueInput>,
) -> ApiResult<Json<WorkQueue>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    let queue = WorkQueueRepository::update(&repo, &tenant.tenant_id, &queue_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkQueue,
        id.clone(), format!("Updated work queue '{}'", queue.name),
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(queue))
}

#[utoipa::path(delete, path = "/api/v1/workflows/queues/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Queue deleted")))]
async fn delete_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    WorkQueueRepository::delete(&repo, &tenant.tenant_id, &queue_id).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Delete, ResourceType::WorkQueue,
        id.clone(), "Deleted work queue",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Debug, Deserialize)]
pub struct QueueItemsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[utoipa::path(get, path = "/api/v1/workflows/queues/{id}/items", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Queue items")))]
async fn list_queue_items(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Query(query): Query<QueueItemsQuery>,
) -> ApiResult<Json<Vec<QueueItem>>> {
    let queue_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;
    
    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(50),
    };
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    let result = repo.get_items(&tenant.tenant_id, &queue_id, &pagination).await?;
    Ok(Json(result.data))
}

#[utoipa::path(post, path = "/api/v1/workflows/queues/{id}/items/{item_id}/claim", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,), ("item_id" = String, Path,)), responses((status = 200, description = "Item claimed")))]
async fn claim_item(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path((_queue_id, item_id)): Path<(String, String)>,
) -> ApiResult<Json<QueueItem>> {
    let item_uuid = uuid::Uuid::parse_str(&item_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid item ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    let item = repo.claim_item(&tenant.tenant_id, item_uuid, &user.user_id).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Claimed, ResourceType::WorkQueue,
        item_id.clone(), "Claimed queue item",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(item))
}

#[derive(Debug, Deserialize)]
pub struct CompleteItemInput {
    pub action: String,
}

#[utoipa::path(post, path = "/api/v1/workflows/queues/{id}/items/{item_id}/complete", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,), ("item_id" = String, Path,)), responses((status = 200, description = "Item completed")))]
async fn complete_item(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path((_queue_id, item_id)): Path<(String, String)>,
    Json(input): Json<CompleteItemInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let item_uuid = uuid::Uuid::parse_str(&item_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid item ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    repo.complete_item(&tenant.tenant_id, item_uuid, &input.action).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkQueue,
        item_id.clone(), format!("Completed queue item with action '{}'", input.action),
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Assignment Rule Handlers
// ============================================================================

#[utoipa::path(get, path = "/api/v1/workflows/assignment-rules", tag = "Workflows", responses((status = 200, description = "Assignment rules")))]
async fn list_assignment_rules(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<AssignmentRule>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool.clone());
    let rules = AssignmentRuleRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(rules))
}

#[utoipa::path(get, path = "/api/v1/workflows/assignment-rules/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Assignment rule")))]
async fn get_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<AssignmentRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;
    
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool.clone());
    let rule = AssignmentRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "AssignmentRule".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(rule))
}

#[utoipa::path(post, path = "/api/v1/workflows/assignment-rules", tag = "Workflows", request_body = serde_json::Value, responses((status = 200, description = "Rule created")))]
async fn create_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateAssignmentRuleInput>,
) -> ApiResult<Json<AssignmentRule>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool.clone());
    let rule = AssignmentRuleRepository::create(&repo, &tenant.tenant_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Create, ResourceType::AssignmentRule,
        rule.id.to_string(), format!("Created assignment rule '{}'", rule.name),
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(rule))
}

#[utoipa::path(put, path = "/api/v1/workflows/assignment-rules/{id}", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Rule updated")))]
async fn update_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateAssignmentRuleInput>,
) -> ApiResult<Json<AssignmentRule>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool.clone());

    // Fetch before-state for SOX audit trail
    let old_rule = AssignmentRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "AssignmentRule".to_string(),
            id: id.clone(),
        })?;

    let rule = AssignmentRuleRepository::update(&repo, &tenant.tenant_id, &rule_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::AssignmentRule,
        id.clone(), format!("Updated assignment rule '{}'", rule.name),
    ).with_user_email(&user.email)
     .with_old_value(serde_json::to_value(&old_rule).unwrap_or(serde_json::Value::Null))
     .with_new_value(serde_json::to_value(&rule).unwrap_or(serde_json::Value::Null));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(rule))
}

#[utoipa::path(delete, path = "/api/v1/workflows/assignment-rules/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Rule deleted")))]
async fn delete_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool.clone());
    AssignmentRuleRepository::delete(&repo, &tenant.tenant_id, &rule_id).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Delete, ResourceType::AssignmentRule,
        id.clone(), "Deleted assignment rule",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Shared Approval Aggregation Logic
// ============================================================================

/// After an individual approval_request is resolved, check if the invoice's
/// overall approval status should change. Only transitions the invoice when
/// ALL approval requests are resolved (no pending remain).
///
/// Returns `Ok(Some(status))` when the invoice status was updated,
/// `Ok(None)` when pending requests remain (no status change).
pub(crate) async fn resolve_invoice_approval_status(
    executor: &mut sqlx::PgConnection,
    tenant_id: &billforge_core::TenantId,
    invoice_id: uuid::Uuid,
) -> Result<Option<billforge_core::domain::ProcessingStatus>, billforge_core::Error> {
    #[derive(sqlx::FromRow)]
    struct Counts {
        pending_count: i64,
        rejected_count: i64,
        approved_count: i64,
    }

    let counts = sqlx::query_as::<_, Counts>(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'pending') AS pending_count,
            COUNT(*) FILTER (WHERE status = 'rejected') AS rejected_count,
            COUNT(*) FILTER (WHERE status = 'approved') AS approved_count
        FROM approval_requests
        WHERE invoice_id = $1 AND tenant_id = $2
        "#,
    )
    .bind(invoice_id)
    .bind(*tenant_id.as_uuid())
    .fetch_one(&mut *executor)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to count approval statuses: {}", e)))?;

    if counts.pending_count > 0 {
        // Still waiting on some approvers - do not change invoice status
        return Ok(None);
    }

    // All requests resolved. Determine final status.
    let new_status = if counts.rejected_count > 0 {
        billforge_core::domain::ProcessingStatus::Rejected
    } else if counts.approved_count > 0 {
        billforge_core::domain::ProcessingStatus::Approved
    } else {
        // No requests exist at all (shouldn't normally happen)
        return Ok(None);
    };

    sqlx::query("UPDATE invoices SET processing_status = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3")
        .bind(new_status.as_str())
        .bind(invoice_id)
        .bind(*tenant_id.as_uuid())
        .execute(&mut *executor)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to update invoice status: {}", e)))?;

    Ok(Some(new_status))
}

// ============================================================================
// ERP Sync Enqueue (on approval completion)
// ============================================================================

/// Pure decision function: returns true only when the invoice just transitioned
/// to Approved status, indicating an ERP sync job should be enqueued.
pub(crate) fn should_enqueue_erp_sync(new_status: &Option<billforge_core::domain::ProcessingStatus>) -> bool {
    matches!(new_status, Some(billforge_core::domain::ProcessingStatus::Approved))
}

/// Enqueue a QuickBooks invoice export job to the Redis job queue.
/// Modeled on `enqueue_ocr_job` in invoices.rs. Redis failure is non-fatal:
/// logs the error and returns Ok so the approval still succeeds.
async fn enqueue_erp_sync_job(
    redis_client: &redis::Client,
    tenant_id: &billforge_core::TenantId,
    invoice_id: Uuid,
) -> Result<(), billforge_core::Error> {
    let job = serde_json::json!({
        "id": Uuid::new_v4().to_string(),
        "job_type": "quick_books_invoice_export",
        "tenant_id": tenant_id.to_string(),
        "payload": {
            "invoice_id": invoice_id.to_string(),
        },
        "created_at": chrono::Utc::now(),
        "retry_count": 0,
    });

    let job_json = match serde_json::to_string(&job) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize ERP sync job — swallowing");
            return Ok(());
        }
    };

    match redis_client.get_async_connection().await {
        Ok(mut conn) => {
            match conn.lpush::<_, _, ()>("billforge:jobs:queue", &job_json).await {
                Ok(()) => {
                    tracing::info!(%invoice_id, "ERP sync job enqueued for approved invoice");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to LPUSH ERP sync job — swallowing");
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Redis connection failed for ERP sync enqueue — swallowing");
        }
    }

    Ok(())
}

/// Wrapper that calls `resolve_invoice_approval_status` and, if the invoice
/// just transitioned to Approved, enqueues an ERP sync job via Redis.
/// The `redis_client` is `Option` so callers without Redis still work.
/// IMPORTANT: call this AFTER `tx.commit()` so Redis work happens outside
/// the DB transaction and the invoice state is durable before the worker wakes.
#[allow(dead_code)]
pub(crate) async fn resolve_invoice_approval_status_and_enqueue_sync(
    executor: &mut sqlx::PgConnection,
    tenant_id: &billforge_core::TenantId,
    invoice_id: Uuid,
    redis_client: Option<&redis::Client>,
) -> Result<Option<billforge_core::domain::ProcessingStatus>, billforge_core::Error> {
    let new_status = resolve_invoice_approval_status(executor, tenant_id, invoice_id).await?;

    if should_enqueue_erp_sync(&new_status) {
        if let Some(client) = redis_client {
            // Non-fatal: fire-and-forget; errors are logged and swallowed inside.
            let _ = enqueue_erp_sync_job(client, tenant_id, invoice_id).await;
        }
    }

    Ok(new_status)
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

#[utoipa::path(get, path = "/api/v1/workflows/approvals/pending", tag = "Workflows", responses((status = 200, description = "Pending approvals")))]
async fn list_pending_approvals(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<PendingApprovalResponse>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Query pending approval requests with invoice details
    #[derive(sqlx::FromRow)]
    struct ApprovalRow {
        id: uuid::Uuid,
        invoice_id: uuid::Uuid,
        status: String,
        created_at: chrono::DateTime<chrono::Utc>,
        invoice_number: Option<String>,
        vendor_name: Option<String>,
        total_amount_cents: Option<i64>,
    }

    let rows = sqlx::query_as::<_, ApprovalRow>(
        r#"
        SELECT
            ar.id,
            ar.invoice_id,
            ar.status,
            ar.created_at,
            i.invoice_number,
            i.vendor_name,
            i.total_amount_cents
        FROM approval_requests ar
        LEFT JOIN invoices i ON ar.invoice_id = i.id
        WHERE ar.tenant_id = $1
          AND ar.status = 'pending'
          AND (ar.expires_at IS NULL OR ar.expires_at > NOW())
        ORDER BY ar.created_at DESC
        "#
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch pending approvals: {}", e)))?;

    let approvals = rows
        .into_iter()
        .map(|row| PendingApprovalResponse {
            id: row.id.to_string(),
            invoice_id: row.invoice_id.to_string(),
            status: row.status,
            created_at: row.created_at.to_rfc3339(),
            invoice_number: row.invoice_number,
            vendor_name: row.vendor_name,
            total_amount: row.total_amount_cents.map(|cents| cents as f64 / 100.0),
        })
        .collect();

    Ok(Json(approvals))
}

#[utoipa::path(get, path = "/api/v1/workflows/approvals/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Approval details")))]
async fn get_approval(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let approval_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    #[derive(sqlx::FromRow)]
    struct ApprovalDetails {
        id: uuid::Uuid,
        invoice_id: uuid::Uuid,
        rule_id: Option<uuid::Uuid>,
        status: String,
        requested_from: serde_json::Value,
        comments: Option<String>,
        responded_by: Option<uuid::Uuid>,
        responded_at: Option<chrono::DateTime<chrono::Utc>>,
        created_at: chrono::DateTime<chrono::Utc>,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        invoice_number: Option<String>,
        vendor_name: Option<String>,
        total_amount_cents: Option<i64>,
        invoice_status: Option<String>,
    }

    let details = sqlx::query_as::<_, ApprovalDetails>(
        r#"
        SELECT
            ar.id,
            ar.invoice_id,
            ar.rule_id,
            ar.status,
            ar.requested_from,
            ar.comments,
            ar.responded_by,
            ar.responded_at,
            ar.created_at,
            ar.expires_at,
            i.invoice_number,
            i.vendor_name,
            i.total_amount_cents,
            i.processing_status as invoice_status
        FROM approval_requests ar
        LEFT JOIN invoices i ON ar.invoice_id = i.id
        WHERE ar.id = $1 AND ar.tenant_id = $2
        "#
    )
    .bind(approval_id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch approval: {}", e)))?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "ApprovalRequest".to_string(),
        id: id.clone(),
    })?;

    let response = serde_json::json!({
        "id": details.id.to_string(),
        "invoice_id": details.invoice_id.to_string(),
        "rule_id": details.rule_id.map(|id| id.to_string()),
        "status": details.status,
        "requested_from": details.requested_from,
        "comments": details.comments,
        "responded_by": details.responded_by.map(|id| id.to_string()),
        "responded_at": details.responded_at.map(|t| t.to_rfc3339()),
        "created_at": details.created_at.to_rfc3339(),
        "expires_at": details.expires_at.map(|t| t.to_rfc3339()),
        "invoice": {
            "invoice_number": details.invoice_number,
            "vendor_name": details.vendor_name,
            "total_amount": details.total_amount_cents.map(|cents| cents as f64 / 100.0),
            "status": details.invoice_status,
        }
    });

    Ok(Json(response))
}

#[derive(Deserialize)]
pub struct ApprovalInput {
    pub comments: Option<String>,
}

#[utoipa::path(post, path = "/api/v1/workflows/approvals/{id}/approve", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Invoice approved")))]
async fn approve(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<ApprovalInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let approval_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval ID".to_string()))?;

    // Get approval request and invoice details
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get approval request and related invoice (including vendor_id and department for limit checks)
    #[derive(sqlx::FromRow)]
    struct ApprovalInfo {
        invoice_id: uuid::Uuid,
        invoice_number: String,
        vendor_name: String,
        total_amount_cents: i64,
        vendor_id: Option<uuid::Uuid>,
        department: Option<String>,
        submitter_email: Option<String>,
    }

    let info = sqlx::query_as::<_, ApprovalInfo>(
        r#"SELECT
            ar.invoice_id,
            i.invoice_number,
            COALESCE(i.vendor_name, 'Unknown') as vendor_name,
            COALESCE(i.total_amount_cents, 0) as total_amount_cents,
            i.vendor_id,
            i.department,
            (SELECT email FROM users WHERE id = i.created_by LIMIT 1) as submitter_email
        FROM approval_requests ar
        JOIN invoices i ON ar.invoice_id = i.id
        WHERE ar.id = $1"#
    )
    .bind(approval_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Database error: {}", e)))?
    .ok_or_else(|| billforge_core::Error::Database("Approval request not found".to_string()))?;

    // Enforce approval limits for the approving user
    #[derive(sqlx::FromRow)]
    struct ApprovalLimitRow {
        max_amount_cents: i64,
        vendor_restrictions: Option<serde_json::Value>,
        department_restrictions: Option<serde_json::Value>,
    }

    let limit = sqlx::query_as::<_, ApprovalLimitRow>(
        "SELECT max_amount_cents, vendor_restrictions, department_restrictions \
         FROM approval_limits WHERE user_id = $1 AND tenant_id = $2"
    )
    .bind(user.user_id.as_uuid())
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to check approval limits: {}", e)))?;

    if let Some(limit) = limit {
        if info.total_amount_cents > limit.max_amount_cents {
            return Err(billforge_core::Error::Forbidden(
                format!("Invoice amount ${:.2} exceeds your approval limit of ${:.2}",
                    info.total_amount_cents as f64 / 100.0,
                    limit.max_amount_cents as f64 / 100.0)
            ).into());
        }
        // Check vendor restrictions if set
        if let Some(ref vendor_restr) = limit.vendor_restrictions {
            if let Some(invoice_vendor_id) = info.vendor_id {
                let allowed_vendors: Vec<uuid::Uuid> = match vendor_restr {
                    serde_json::Value::Array(arr) => {
                        arr.iter()
                            .filter_map(|v| v.as_str().and_then(|s| uuid::Uuid::parse_str(s).ok()))
                            .collect()
                    }
                    _ => vec![],
                };
                if !allowed_vendors.is_empty() && !allowed_vendors.contains(&invoice_vendor_id) {
                    return Err(billforge_core::Error::Forbidden(
                        "You are not authorized to approve invoices from this vendor".to_string(),
                    ).into());
                }
            }
        }
        // Check department restrictions if set
        if let Some(ref dept_restr) = limit.department_restrictions {
            if let Some(ref invoice_dept) = info.department {
                let allowed_depts: Vec<String> = match dept_restr {
                    serde_json::Value::Array(arr) => {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    }
                    _ => vec![],
                };
                if !allowed_depts.is_empty() && !allowed_depts.contains(invoice_dept) {
                    return Err(billforge_core::Error::Forbidden(
                        "You are not authorized to approve invoices for this department".to_string(),
                    ).into());
                }
            }
        }
    }

    // Update approval request and invoice status in a transaction
    let mut tx = pool.begin().await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to begin transaction: {}", e)))?;

    // Capture before-state for SOX audit trail (inside transaction for consistency)
    #[derive(sqlx::FromRow)]
    struct ApprovalBeforeState {
        status: String,
        responded_by: Option<uuid::Uuid>,
        responded_at: Option<chrono::DateTime<chrono::Utc>>,
        comments: Option<String>,
    }
    let old_approval = sqlx::query_as::<_, ApprovalBeforeState>(
        "SELECT status, responded_by, responded_at, comments FROM approval_requests WHERE id = $1"
    )
    .bind(approval_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update approval request status (only if still pending)
    let updated = sqlx::query_scalar::<_, uuid::Uuid>(
        "UPDATE approval_requests SET status = 'approved', responded_by = $1, responded_at = NOW(), comments = $2 WHERE id = $3 AND status = 'pending' RETURNING id"
    )
    .bind(user.user_id.as_uuid())
    .bind(&input.comments)
    .bind(approval_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    if updated.is_none() {
        tx.rollback().await.map_err(|e| billforge_core::Error::Database(e.to_string()))?;
        return Err(billforge_core::Error::Conflict(
            "Approval request has already been processed".to_string(),
        ).into());
    }

    // Resolve invoice approval status (only transitions if ALL requests resolved)
    let new_status = resolve_invoice_approval_status(&mut *tx, &tenant.tenant_id, info.invoice_id).await?;

    tx.commit().await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to commit transaction: {}", e)))?;

    // Enqueue ERP sync if invoice just transitioned to Approved (after commit so state is durable)
    if should_enqueue_erp_sync(&new_status) {
        if let Some(ref redis_client) = state.redis {
            let _ = enqueue_erp_sync_job(redis_client, &tenant.tenant_id, info.invoice_id).await;
        }
    }

    // Send email notification to submitter
    if let Some(submitter_email) = info.submitter_email {
        let approver_name = user.email.clone(); // Use email as name for now
        let amount_formatted = format!("${:.2}", info.total_amount_cents as f64 / 100.0);

        let (html, text) = EmailTemplates::invoice_approved(
            &info.invoice_number,
            &info.vendor_name,
            &amount_formatted,
            &approver_name,
        );

        // Send email in background (don't block the response)
        let email_service = state.email.clone();
        let subject = format!("Invoice {} Approved", info.invoice_number);
        tokio::spawn(async move {
            if let Err(e) = email_service.send(&submitter_email, &subject, &html, &text).await {
                tracing::error!("Failed to send approval notification email: {}", e);
            }
        });
    }

    let old_value = old_approval.map(|oa| serde_json::json!({
        "status": oa.status,
        "responded_by": oa.responded_by.map(|u| u.to_string()),
        "responded_at": oa.responded_at.map(|t| t.to_rfc3339()),
        "comments": oa.comments,
    })).unwrap_or(serde_json::Value::Null);

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::InvoiceApproved, ResourceType::ApprovalRequest,
        id.clone(),
        format!("Approved invoice {}", info.invoice_number),
    ).with_user_email(&user.email)
     .with_old_value(old_value)
     .with_new_value(serde_json::json!({
         "status": "approved",
         "responded_by": user.user_id.to_string(),
         "responded_at": chrono::Utc::now().to_rfc3339(),
         "comments": input.comments,
     }))
     .with_metadata(serde_json::json!({
         "invoice_id": info.invoice_id.to_string(),
         "comments": input.comments,
     }));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(serde_json::json!({
        "message": "Approved",
        "approval_id": id,
        "invoice_id": info.invoice_id.to_string(),
        "approved_by": user.user_id.0.to_string()
    })))
}

#[utoipa::path(post, path = "/api/v1/workflows/approvals/{id}/reject", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Invoice rejected")))]
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Get approval request and related invoice
    #[derive(sqlx::FromRow)]
    struct ApprovalInfo {
        invoice_id: uuid::Uuid,
        invoice_number: String,
        vendor_name: String,
        total_amount_cents: i64,
        submitter_email: Option<String>,
    }

    let info = sqlx::query_as::<_, ApprovalInfo>(
        r#"SELECT
            ar.invoice_id,
            i.invoice_number,
            COALESCE(i.vendor_name, 'Unknown') as vendor_name,
            COALESCE(i.total_amount_cents, 0) as total_amount_cents,
            (SELECT email FROM users WHERE id = i.created_by LIMIT 1) as submitter_email
        FROM approval_requests ar
        JOIN invoices i ON ar.invoice_id = i.id
        WHERE ar.id = $1"#
    )
    .bind(approval_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Database error: {}", e)))?
    .ok_or_else(|| billforge_core::Error::Database("Approval request not found".to_string()))?;

    // Update approval request and invoice status in a transaction
    let mut tx = pool.begin().await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to begin transaction: {}", e)))?;

    // Capture before-state for SOX audit trail (inside transaction for consistency)
    #[derive(sqlx::FromRow)]
    struct ApprovalBeforeState {
        status: String,
        responded_by: Option<uuid::Uuid>,
        responded_at: Option<chrono::DateTime<chrono::Utc>>,
        comments: Option<String>,
    }
    let old_approval = sqlx::query_as::<_, ApprovalBeforeState>(
        "SELECT status, responded_by, responded_at, comments FROM approval_requests WHERE id = $1"
    )
    .bind(approval_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update approval request status (only if still pending)
    let updated = sqlx::query_scalar::<_, uuid::Uuid>(
        "UPDATE approval_requests SET status = 'rejected', responded_by = $1, responded_at = NOW(), comments = $2 WHERE id = $3 AND status = 'pending' RETURNING id"
    )
    .bind(user.user_id.as_uuid())
    .bind(&reason)
    .bind(approval_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    if updated.is_none() {
        tx.rollback().await.map_err(|e| billforge_core::Error::Database(e.to_string()))?;
        return Err(billforge_core::Error::Conflict(
            "Approval request has already been processed".to_string(),
        ).into());
    }

    // Resolve invoice approval status (only transitions if ALL requests resolved)
    let new_status = resolve_invoice_approval_status(&mut *tx, &tenant.tenant_id, info.invoice_id).await?;

    tx.commit().await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to commit transaction: {}", e)))?;

    // Enqueue ERP sync if invoice just transitioned to Approved (after commit so state is durable).
    // This will not fire for rejections, but the pattern is kept consistent with the approve handler.
    if should_enqueue_erp_sync(&new_status) {
        if let Some(ref redis_client) = state.redis {
            let _ = enqueue_erp_sync_job(redis_client, &tenant.tenant_id, info.invoice_id).await;
        }
    }

    // Send email notification to submitter
    if let Some(submitter_email) = info.submitter_email {
        let rejecter_name = user.email.clone();
        let amount_formatted = format!("${:.2}", info.total_amount_cents as f64 / 100.0);

        let (html, text) = EmailTemplates::invoice_rejected(
            &info.invoice_number,
            &info.vendor_name,
            &amount_formatted,
            &rejecter_name,
            &reason,
        );

        // Send email in background
        let email_service = state.email.clone();
        let subject = format!("Invoice {} Rejected", info.invoice_number);
        tokio::spawn(async move {
            if let Err(e) = email_service.send(&submitter_email, &subject, &html, &text).await {
                tracing::error!("Failed to send rejection notification email: {}", e);
            }
        });
    }

    let old_value = old_approval.map(|oa| serde_json::json!({
        "status": oa.status,
        "responded_by": oa.responded_by.map(|u| u.to_string()),
        "responded_at": oa.responded_at.map(|t| t.to_rfc3339()),
        "comments": oa.comments,
    })).unwrap_or(serde_json::Value::Null);

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::InvoiceRejected, ResourceType::ApprovalRequest,
        id.clone(),
        format!("Rejected invoice {}", info.invoice_number),
    ).with_user_email(&user.email)
     .with_old_value(old_value)
     .with_new_value(serde_json::json!({
         "status": "rejected",
         "responded_by": user.user_id.to_string(),
         "responded_at": chrono::Utc::now().to_rfc3339(),
         "comments": reason,
     }))
     .with_metadata(serde_json::json!({
         "invoice_id": info.invoice_id.to_string(),
         "reason": reason,
     }));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(serde_json::json!({
        "message": "Rejected",
        "approval_id": id,
        "invoice_id": info.invoice_id.to_string(),
        "rejected_by": user.user_id.0.to_string(),
        "reason": reason
    })))
}

// ============================================================================
// Workflow Template Handlers
// ============================================================================

#[utoipa::path(get, path = "/api/v1/workflows/templates", tag = "Workflows", responses((status = 200, description = "Templates")))]
async fn list_templates(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<WorkflowTemplate>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let templates = WorkflowTemplateRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(templates))
}

#[utoipa::path(get, path = "/api/v1/workflows/templates/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Template details")))]
async fn get_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let template_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let template = WorkflowTemplateRepository::get_by_id(&repo, &tenant.tenant_id, &template_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowTemplate".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(template))
}

#[utoipa::path(post, path = "/api/v1/workflows/templates", tag = "Workflows", request_body = serde_json::Value, responses((status = 200, description = "Template created")))]
async fn create_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateWorkflowTemplateInput>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let template = WorkflowTemplateRepository::create(&repo, &tenant.tenant_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Create, ResourceType::WorkflowTemplate,
        template.id.to_string(), format!("Created workflow template '{}'", template.name),
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(template))
}

#[utoipa::path(put, path = "/api/v1/workflows/templates/{id}", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Template updated")))]
async fn update_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkflowTemplateInput>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let template_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    // Fetch before-state for SOX audit trail
    let old_template = WorkflowTemplateRepository::get_by_id(&repo, &tenant.tenant_id, &template_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowTemplate".to_string(),
            id: id.clone(),
        })?;

    let template = WorkflowTemplateRepository::update(&repo, &tenant.tenant_id, &template_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkflowTemplate,
        id.clone(), format!("Updated workflow template '{}'", template.name),
    ).with_user_email(&user.email)
     .with_old_value(serde_json::to_value(&old_template).unwrap_or(serde_json::Value::Null))
     .with_new_value(serde_json::to_value(&template).unwrap_or(serde_json::Value::Null));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(template))
}

#[utoipa::path(delete, path = "/api/v1/workflows/templates/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Template deleted")))]
async fn delete_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let template_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    WorkflowTemplateRepository::delete(&repo, &tenant.tenant_id, &template_id).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Delete, ResourceType::WorkflowTemplate,
        id.clone(), "Deleted workflow template",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(post, path = "/api/v1/workflows/templates/{id}/activate", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Template activated")))]
async fn activate_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let template_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    WorkflowTemplateRepository::set_active(&repo, &tenant.tenant_id, &template_id, true).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkflowTemplate,
        id.clone(), "Activated workflow template",
    ).with_user_email(&user.email)
     .with_metadata(serde_json::json!({ "active": true }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(post, path = "/api/v1/workflows/templates/{id}/deactivate", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Template deactivated")))]
async fn deactivate_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let template_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    WorkflowTemplateRepository::set_active(&repo, &tenant.tenant_id, &template_id, false).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::WorkflowTemplate,
        id.clone(), "Deactivated workflow template",
    ).with_user_email(&user.email)
     .with_metadata(serde_json::json!({ "active": false }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Bulk Operations Handler
// ============================================================================

#[utoipa::path(post, path = "/api/v1/workflows/bulk", tag = "Workflows", request_body = serde_json::Value, responses((status = 200, description = "Bulk operation result")))]
async fn bulk_operation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<BulkOperationInput>,
) -> ApiResult<Json<BulkOperationResult>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
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
            BulkOperationType::Approve | BulkOperationType::Reject => {
                // Check for pending approval_requests - bulk operations must not
                // bypass the multi-approval workflow resolution logic.
                let has_approval_requests: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM approval_requests WHERE invoice_id = $1 AND tenant_id = $2 AND status = 'pending')"
                )
                .bind(invoice_id.as_uuid())
                .bind(*tenant.tenant_id.as_uuid())
                .fetch_one(&*pool)
                .await
                .map_err(|e| billforge_core::Error::Database(format!(
                    "Failed to check approval_requests for invoice {}: {}", invoice_id, e
                )))?;

                if has_approval_requests {
                    Err(billforge_core::Error::Validation(
                        "Cannot bulk-approve/reject invoice with active approval workflow - use individual approval actions".to_string()
                    ))
                } else {
                    let status = if matches!(input.operation, BulkOperationType::Approve) {
                        billforge_core::domain::ProcessingStatus::Approved
                    } else {
                        billforge_core::domain::ProcessingStatus::Rejected
                    };
                    invoice_repo.update_processing_status(
                        &tenant.tenant_id,
                        invoice_id,
                        status,
                    ).await
                }
            }
            BulkOperationType::MoveToQueue | BulkOperationType::AssignTo => {
                // These require additional parameters - skip for now
                Ok(())
            }
        };

        match result {
            Ok(_) => {
                successful += 1;
                let action = match input.operation {
                    BulkOperationType::SubmitForPayment => AuditAction::InvoiceMarkedForPayment,
                    BulkOperationType::Approve => AuditAction::InvoiceApproved,
                    BulkOperationType::Reject => AuditAction::InvoiceRejected,
                    _ => AuditAction::Update,
                };
                let audit_entry = AuditEntry::new(
                    tenant.tenant_id.clone(), Some(user.user_id.clone()),
                    action, ResourceType::Invoice,
                    invoice_id.to_string(),
                    format!("Bulk {:?}", input.operation),
                ).with_user_email(&user.email)
                 .with_metadata(serde_json::json!({ "bulk": true }));
                let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
                if let Err(e) = audit_repo.log(audit_entry).await {
                    tracing::warn!(error = %e, "Failed to log audit entry");
                }
            }
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

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct HoldInput {
    pub reason: String,
}

#[utoipa::path(post, path = "/api/v1/workflows/invoices/{id}/hold", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Invoice on hold")))]
async fn put_on_hold(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(_input): Json<HoldInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::OnHold,
    ).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::InvoicePutOnHold, ResourceType::Invoice,
        id.clone(), "Put invoice on hold",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "message": "Invoice placed on hold" })))
}

#[utoipa::path(post, path = "/api/v1/workflows/invoices/{id}/release", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Invoice released")))]
async fn release_from_hold(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::Submitted,
    ).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::InvoiceReleasedFromHold, ResourceType::Invoice,
        id.clone(), "Released invoice from hold",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "message": "Invoice released from hold" })))
}

#[utoipa::path(post, path = "/api/v1/workflows/invoices/{id}/void", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Invoice voided")))]
async fn void_invoice(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::Voided,
    ).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::InvoiceVoided, ResourceType::Invoice,
        id.clone(), "Voided invoice",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "message": "Invoice voided" })))
}

#[utoipa::path(post, path = "/api/v1/workflows/invoices/{id}/ready-for-payment", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Marked ready for payment")))]
async fn mark_ready_for_payment(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id.parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::ReadyForPayment,
    ).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::InvoiceMarkedForPayment, ResourceType::Invoice,
        id.clone(), "Marked invoice ready for payment",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "message": "Invoice marked ready for payment" })))
}

#[derive(Debug, Deserialize)]
pub struct MoveToQueueInput {
    pub queue_id: String,
    pub assign_to: Option<String>,
}

#[utoipa::path(post, path = "/api/v1/workflows/invoices/{id}/move-to-queue", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Moved to queue")))]
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

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool.clone());
    let item = repo.move_item(&tenant.tenant_id, &invoice_id, &queue_id, assign_to.as_ref()).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::MovedToQueue, ResourceType::Invoice,
        id.clone(), "Moved invoice to queue",
    ).with_user_email(&user.email)
     .with_metadata(serde_json::json!({ "queue_id": input.queue_id, "assign_to": input.assign_to }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(item))
}

// ============================================================================
// Approval Delegation Handlers
// ============================================================================

/// Validates a delegation creation/update request:
/// 1. Basic field checks (UUID validity, non-self, date order)
/// 2. Both users exist in the tenant
/// 3. No circular delegation chain would be formed
async fn validate_delegation_input(
    pool: &sqlx::PgPool,
    tenant_id: &billforge_core::types::TenantId,
    input: &CreateApprovalDelegationInput,
    exclude_delegation_id: Option<Uuid>,
) -> crate::error::ApiResult<()> {
    // 1. Basic validation (UUIDs, self-delegation, date order)
    input.validate_basic()?;

    let delegator_uuid = Uuid::parse_str(&input.delegator_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid delegator_id".to_string()))?;
    let delegate_uuid = Uuid::parse_str(&input.delegate_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid delegate_id".to_string()))?;
    let tenant_uuid = *tenant_id.as_uuid();

    // 2. Verify both users exist in the tenant
    let delegator_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND tenant_id = $2)"
    )
    .bind(delegator_uuid)
    .bind(tenant_uuid)
    .fetch_one(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to verify delegator: {}", e)))?;

    if !delegator_exists {
        return Err(billforge_core::Error::Validation(
            "Delegator user not found in tenant".to_string(),
        ).into());
    }

    let delegate_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1 AND tenant_id = $2)"
    )
    .bind(delegate_uuid)
    .bind(tenant_uuid)
    .fetch_one(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to verify delegate: {}", e)))?;

    if !delegate_exists {
        return Err(billforge_core::Error::Validation(
            "Delegate user not found in tenant".to_string(),
        ).into());
    }

    // 3. Circular chain detection
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(
        std::sync::Arc::new(pool.clone()),
    );
    let delegations = billforge_core::traits::ApprovalDelegationRepository::list(&repo, tenant_id).await?;

    let active: Vec<ApprovalDelegation> = delegations
        .into_iter()
        .filter(|d| {
            d.is_active && exclude_delegation_id.map_or(true, |exclude| d.id != exclude)
        })
        .collect();

    let delegator_user_id = billforge_core::types::UserId::from_uuid(delegator_uuid);
    let delegate_user_id = billforge_core::types::UserId::from_uuid(delegate_uuid);

    if let Some(cycle) = detect_delegation_cycle(
        &active,
        &delegator_user_id,
        &delegate_user_id,
        input.start_date,
        input.end_date,
    ) {
        let path_str = cycle
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(billforge_core::Error::Validation(
            format!("Circular delegation chain detected: {} -> {}", path_str, delegator_user_id),
        ).into());
    }

    Ok(())
}

#[utoipa::path(get, path = "/api/v1/workflows/delegations", tag = "Workflows", responses((status = 200, description = "Delegations")))]
async fn list_delegations(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<ApprovalDelegation>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let delegations = ApprovalDelegationRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(delegations))
}

#[utoipa::path(get, path = "/api/v1/workflows/delegations/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Delegation details")))]
async fn get_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<ApprovalDelegation>> {
    let delegation_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid delegation ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let delegation = ApprovalDelegationRepository::get_by_id(&repo, &tenant.tenant_id, delegation_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ApprovalDelegation".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(delegation))
}

#[utoipa::path(post, path = "/api/v1/workflows/delegations", tag = "Workflows", request_body = serde_json::Value, responses((status = 200, description = "Delegation created")))]
async fn create_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateApprovalDelegationInput>,
) -> ApiResult<Json<ApprovalDelegation>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    validate_delegation_input(&pool, &tenant.tenant_id, &input, None).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let delegation = ApprovalDelegationRepository::create(&repo, &tenant.tenant_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Create, ResourceType::ApprovalDelegation,
        delegation.id.to_string(), "Created approval delegation",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(delegation))
}

#[utoipa::path(put, path = "/api/v1/workflows/delegations/{id}", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Delegation updated")))]
async fn update_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateApprovalDelegationInput>,
) -> ApiResult<Json<ApprovalDelegation>> {
    let delegation_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid delegation ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    validate_delegation_input(&pool, &tenant.tenant_id, &input, Some(delegation_id)).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    // Fetch before-state for SOX audit trail
    let old_delegation = ApprovalDelegationRepository::get_by_id(&repo, &tenant.tenant_id, delegation_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ApprovalDelegation".to_string(),
            id: id.clone(),
        })?;

    let delegation = ApprovalDelegationRepository::update(&repo, &tenant.tenant_id, delegation_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::ApprovalDelegation,
        id.clone(), "Updated approval delegation",
    ).with_user_email(&user.email)
     .with_old_value(serde_json::to_value(&old_delegation).unwrap_or(serde_json::Value::Null))
     .with_new_value(serde_json::to_value(&delegation).unwrap_or(serde_json::Value::Null));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(delegation))
}

#[utoipa::path(delete, path = "/api/v1/workflows/delegations/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Delegation deleted")))]
async fn delete_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let delegation_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid delegation ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    ApprovalDelegationRepository::delete(&repo, &tenant.tenant_id, delegation_id).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Delete, ResourceType::ApprovalDelegation,
        id.clone(), "Deleted approval delegation",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Approval Limit Handlers
// ============================================================================

#[utoipa::path(get, path = "/api/v1/workflows/approval-limits", tag = "Workflows", responses((status = 200, description = "Approval limits")))]
async fn list_approval_limits(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<ApprovalLimit>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let limits = ApprovalLimitRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(limits))
}

#[utoipa::path(get, path = "/api/v1/workflows/approval-limits/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Approval limit")))]
async fn get_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<ApprovalLimit>> {
    let limit_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval limit ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let limit = ApprovalLimitRepository::get_by_id(&repo, &tenant.tenant_id, limit_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ApprovalLimit".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(limit))
}

#[utoipa::path(post, path = "/api/v1/workflows/approval-limits", tag = "Workflows", request_body = serde_json::Value, responses((status = 200, description = "Limit created")))]
async fn create_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateApprovalLimitInput>,
) -> ApiResult<Json<ApprovalLimit>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    let limit = ApprovalLimitRepository::create(&repo, &tenant.tenant_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Create, ResourceType::ApprovalLimit,
        limit.id.to_string(), "Created approval limit",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(limit))
}

#[utoipa::path(put, path = "/api/v1/workflows/approval-limits/{id}", tag = "Workflows", request_body = serde_json::Value, params(("id" = String, Path,)), responses((status = 200, description = "Limit updated")))]
async fn update_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateApprovalLimitInput>,
) -> ApiResult<Json<ApprovalLimit>> {
    let limit_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval limit ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    // Fetch before-state for SOX audit trail
    let old_limit = ApprovalLimitRepository::get_by_id(&repo, &tenant.tenant_id, limit_id).await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ApprovalLimit".to_string(),
            id: id.clone(),
        })?;

    let limit = ApprovalLimitRepository::update(&repo, &tenant.tenant_id, limit_id, input).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Update, ResourceType::ApprovalLimit,
        id.clone(), "Updated approval limit",
    ).with_user_email(&user.email)
     .with_old_value(serde_json::to_value(&old_limit).unwrap_or(serde_json::Value::Null))
     .with_new_value(serde_json::to_value(&limit).unwrap_or(serde_json::Value::Null));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(limit))
}

#[utoipa::path(delete, path = "/api/v1/workflows/approval-limits/{id}", tag = "Workflows", params(("id" = String, Path,)), responses((status = 200, description = "Limit deleted")))]
async fn delete_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let limit_id = id.parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval limit ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());
    ApprovalLimitRepository::delete(&repo, &tenant.tenant_id, limit_id).await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(), Some(user.user_id.clone()),
        AuditAction::Delete, ResourceType::ApprovalLimit,
        id.clone(), "Deleted approval limit",
    ).with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}
