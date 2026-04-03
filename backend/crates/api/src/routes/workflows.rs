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
        ApprovalDelegation, ApprovalLimit, AssignmentRule, BulkOperationError, BulkOperationInput,
        BulkOperationResult, BulkOperationType, CreateApprovalDelegationInput,
        CreateApprovalLimitInput, CreateAssignmentRuleInput, CreateWorkQueueInput,
        CreateWorkflowRuleInput, CreateWorkflowTemplateInput, QueueItem, WorkQueue, WorkflowRule,
        WorkflowTemplate,
    },
    traits::{
        ApprovalDelegationRepository, ApprovalLimitRepository, AssignmentRuleRepository,
        InvoiceRepository, WorkQueueRepository, WorkflowRuleRepository, WorkflowTemplateRepository,
    },
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
        .route(
            "/invoices/:id/ready-for-payment",
            post(mark_ready_for_payment),
        )
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let rules =
        billforge_core::traits::WorkflowRuleRepository::list(&repo, &tenant.tenant_id, None)
            .await?;
    Ok(Json(rules))
}

async fn get_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowRule>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let rule = billforge_core::traits::WorkflowRuleRepository::get_by_id(
        &repo,
        &tenant.tenant_id,
        &rule_id,
    )
    .await?
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let rule = WorkflowRuleRepository::create(&repo, &tenant.tenant_id, input).await?;
    Ok(Json(rule))
}

async fn update_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkflowRuleInput>,
) -> ApiResult<Json<WorkflowRule>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let rule = WorkflowRuleRepository::update(&repo, &tenant.tenant_id, &rule_id, input).await?;
    Ok(Json(rule))
}

async fn delete_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    WorkflowRuleRepository::delete(&repo, &tenant.tenant_id, &rule_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn activate_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    WorkflowRuleRepository::set_active(&repo, &tenant.tenant_id, &rule_id, true).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn deactivate_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    WorkflowRuleRepository::set_active(&repo, &tenant.tenant_id, &rule_id, false).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Work Queue Handlers
// ============================================================================

async fn list_queues(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<WorkQueue>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    let queues = WorkQueueRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(queues))
}

async fn get_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkQueue>> {
    let queue_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    let queue = WorkQueueRepository::get_by_id(&repo, &tenant.tenant_id, &queue_id)
        .await?
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    let queue = WorkQueueRepository::create(&repo, &tenant.tenant_id, input).await?;
    Ok(Json(queue))
}

async fn update_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkQueueInput>,
) -> ApiResult<Json<WorkQueue>> {
    let queue_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    let queue = WorkQueueRepository::update(&repo, &tenant.tenant_id, &queue_id, input).await?;
    Ok(Json(queue))
}

async fn delete_queue(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let queue_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    WorkQueueRepository::delete(&repo, &tenant.tenant_id, &queue_id).await?;
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
    let queue_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;

    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(50),
    };

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    let result = repo
        .get_items(&tenant.tenant_id, &queue_id, &pagination)
        .await?;
    Ok(Json(result.data))
}

async fn claim_item(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path((queue_id, item_id)): Path<(String, String)>,
) -> ApiResult<Json<QueueItem>> {
    let item_uuid = uuid::Uuid::parse_str(&item_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid item ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    let item = repo
        .claim_item(&tenant.tenant_id, item_uuid, &user.user_id)
        .await?;
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

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    repo.complete_item(&tenant.tenant_id, item_uuid, &input.action)
        .await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Assignment Rule Handlers
// ============================================================================

async fn list_assignment_rules(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<AssignmentRule>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool);
    let rules = AssignmentRuleRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(rules))
}

async fn get_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<AssignmentRule>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool);
    let rule = AssignmentRuleRepository::get_by_id(&repo, &tenant.tenant_id, &rule_id)
        .await?
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
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool);
    let rule = AssignmentRuleRepository::create(&repo, &tenant.tenant_id, input).await?;
    Ok(Json(rule))
}

async fn update_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateAssignmentRuleInput>,
) -> ApiResult<Json<AssignmentRule>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool);
    let rule = AssignmentRuleRepository::update(&repo, &tenant.tenant_id, &rule_id, input).await?;
    Ok(Json(rule))
}

async fn delete_assignment_rule(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let rule_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid rule ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::AssignmentRuleRepositoryImpl::new(pool);
    AssignmentRuleRepository::delete(&repo, &tenant.tenant_id, &rule_id).await?;
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
        "#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to fetch pending approvals: {}", e))
    })?;

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

async fn get_approval(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let approval_id = id
        .parse::<uuid::Uuid>()
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
        "#,
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

async fn approve(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<ApprovalInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let approval_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval ID".to_string()))?;

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
        WHERE ar.id = $1"#,
    )
    .bind(approval_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Database error: {}", e)))?
    .ok_or_else(|| billforge_core::Error::Database("Approval request not found".to_string()))?;

    // Update approval request status
    sqlx::query(
        "UPDATE approval_requests SET status = 'approved', responded_by = $1, responded_at = NOW(), comments = $2 WHERE id = $3"
    )
    .bind(user.user_id.as_uuid())
    .bind(&input.comments)
    .bind(approval_id)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update invoice processing status
    let invoice_id_typed = billforge_core::InvoiceId(info.invoice_id);

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    invoice_repo
        .update_processing_status(
            &tenant.tenant_id,
            &invoice_id_typed,
            billforge_core::domain::ProcessingStatus::Approved,
        )
        .await?;

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
            if let Err(e) = email_service
                .send(&submitter_email, &subject, &html, &text)
                .await
            {
                tracing::error!("Failed to send approval notification email: {}", e);
            }
        });
    }

    Ok(Json(serde_json::json!({
        "message": "Approved",
        "approval_id": id,
        "invoice_id": info.invoice_id.to_string(),
        "approved_by": user.user_id.0.to_string()
    })))
}

async fn reject(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<ApprovalInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let approval_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval ID".to_string()))?;

    let reason = input
        .comments
        .clone()
        .unwrap_or_else(|| "No reason provided".to_string());

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
        WHERE ar.id = $1"#,
    )
    .bind(approval_id)
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Database error: {}", e)))?
    .ok_or_else(|| billforge_core::Error::Database("Approval request not found".to_string()))?;

    // Update approval request status
    sqlx::query(
        "UPDATE approval_requests SET status = 'rejected', responded_by = $1, responded_at = NOW(), comments = $2 WHERE id = $3"
    )
    .bind(user.user_id.as_uuid())
    .bind(&reason)
    .bind(approval_id)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    // Update invoice processing status
    let invoice_id_typed = billforge_core::InvoiceId(info.invoice_id);

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    invoice_repo
        .update_processing_status(
            &tenant.tenant_id,
            &invoice_id_typed,
            billforge_core::domain::ProcessingStatus::Rejected,
        )
        .await?;

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
            if let Err(e) = email_service
                .send(&submitter_email, &subject, &html, &text)
                .await
            {
                tracing::error!("Failed to send rejection notification email: {}", e);
            }
        });
    }

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

async fn list_templates(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<WorkflowTemplate>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let templates = WorkflowTemplateRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(templates))
}

async fn get_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let template_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let template = WorkflowTemplateRepository::get_by_id(&repo, &tenant.tenant_id, &template_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "WorkflowTemplate".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(template))
}

async fn create_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateWorkflowTemplateInput>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let template = WorkflowTemplateRepository::create(&repo, &tenant.tenant_id, input).await?;
    Ok(Json(template))
}

async fn update_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateWorkflowTemplateInput>,
) -> ApiResult<Json<WorkflowTemplate>> {
    let template_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let template =
        WorkflowTemplateRepository::update(&repo, &tenant.tenant_id, &template_id, input).await?;
    Ok(Json(template))
}

async fn delete_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let template_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    WorkflowTemplateRepository::delete(&repo, &tenant.tenant_id, &template_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn activate_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let template_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    WorkflowTemplateRepository::set_active(&repo, &tenant.tenant_id, &template_id, true).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

async fn deactivate_template(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let template_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid template ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    WorkflowTemplateRepository::set_active(&repo, &tenant.tenant_id, &template_id, false).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Bulk Operations Handler
// ============================================================================

async fn bulk_operation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<BulkOperationInput>,
) -> ApiResult<Json<BulkOperationResult>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let mut successful = 0;
    let mut errors = Vec::new();

    for invoice_id in &input.invoice_ids {
        let result = match input.operation {
            BulkOperationType::SubmitForPayment => {
                invoice_repo
                    .update_processing_status(
                        &tenant.tenant_id,
                        invoice_id,
                        billforge_core::domain::ProcessingStatus::ReadyForPayment,
                    )
                    .await
            }
            BulkOperationType::Approve => {
                invoice_repo
                    .update_processing_status(
                        &tenant.tenant_id,
                        invoice_id,
                        billforge_core::domain::ProcessingStatus::Approved,
                    )
                    .await
            }
            BulkOperationType::Reject => {
                invoice_repo
                    .update_processing_status(
                        &tenant.tenant_id,
                        invoice_id,
                        billforge_core::domain::ProcessingStatus::Rejected,
                    )
                    .await
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
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::OnHold,
    )
    .await?;

    Ok(Json(
        serde_json::json!({ "message": "Invoice placed on hold" }),
    ))
}

async fn release_from_hold(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::Submitted,
    )
    .await?;

    Ok(Json(
        serde_json::json!({ "message": "Invoice released from hold" }),
    ))
}

async fn void_invoice(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::Voided,
    )
    .await?;

    Ok(Json(serde_json::json!({ "message": "Invoice voided" })))
}

async fn mark_ready_for_payment(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    repo.update_processing_status(
        &tenant.tenant_id,
        &invoice_id,
        billforge_core::domain::ProcessingStatus::ReadyForPayment,
    )
    .await?;

    Ok(Json(
        serde_json::json!({ "message": "Invoice marked ready for payment" }),
    ))
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
    let invoice_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;
    let queue_id = input
        .queue_id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid queue ID".to_string()))?;

    let assign_to = if let Some(ref user_id_str) = input.assign_to {
        Some(billforge_core::UserId(
            uuid::Uuid::parse_str(user_id_str)
                .map_err(|_| billforge_core::Error::Validation("Invalid user ID".to_string()))?,
        ))
    } else {
        None
    };

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkQueueRepositoryImpl::new(pool);
    let item = repo
        .move_item(
            &tenant.tenant_id,
            &invoice_id,
            &queue_id,
            assign_to.as_ref(),
        )
        .await?;

    Ok(Json(item))
}

// ============================================================================
// Approval Delegation Handlers
// ============================================================================

async fn list_delegations(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<ApprovalDelegation>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let delegations = ApprovalDelegationRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(delegations))
}

async fn get_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<ApprovalDelegation>> {
    let delegation_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid delegation ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let delegation =
        ApprovalDelegationRepository::get_by_id(&repo, &tenant.tenant_id, delegation_id)
            .await?
            .ok_or_else(|| billforge_core::Error::NotFound {
                resource_type: "ApprovalDelegation".to_string(),
                id: id.clone(),
            })?;

    Ok(Json(delegation))
}

async fn create_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateApprovalDelegationInput>,
) -> ApiResult<Json<ApprovalDelegation>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let delegation = ApprovalDelegationRepository::create(&repo, &tenant.tenant_id, input).await?;
    Ok(Json(delegation))
}

async fn update_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateApprovalDelegationInput>,
) -> ApiResult<Json<ApprovalDelegation>> {
    let delegation_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid delegation ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let delegation =
        ApprovalDelegationRepository::update(&repo, &tenant.tenant_id, delegation_id, input)
            .await?;
    Ok(Json(delegation))
}

async fn delete_delegation(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let delegation_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid delegation ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    ApprovalDelegationRepository::delete(&repo, &tenant.tenant_id, delegation_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}

// ============================================================================
// Approval Limit Handlers
// ============================================================================

async fn list_approval_limits(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
) -> ApiResult<Json<Vec<ApprovalLimit>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let limits = ApprovalLimitRepository::list(&repo, &tenant.tenant_id).await?;
    Ok(Json(limits))
}

async fn get_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<ApprovalLimit>> {
    let limit_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval limit ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let limit = ApprovalLimitRepository::get_by_id(&repo, &tenant.tenant_id, limit_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ApprovalLimit".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(limit))
}

async fn create_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(input): Json<CreateApprovalLimitInput>,
) -> ApiResult<Json<ApprovalLimit>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let limit = ApprovalLimitRepository::create(&repo, &tenant.tenant_id, input).await?;
    Ok(Json(limit))
}

async fn update_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
    Json(input): Json<CreateApprovalLimitInput>,
) -> ApiResult<Json<ApprovalLimit>> {
    let limit_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval limit ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    let limit = ApprovalLimitRepository::update(&repo, &tenant.tenant_id, limit_id, input).await?;
    Ok(Json(limit))
}

async fn delete_approval_limit(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let limit_id = id
        .parse::<uuid::Uuid>()
        .map_err(|_| billforge_core::Error::Validation("Invalid approval limit ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool);
    ApprovalLimitRepository::delete(&repo, &tenant.tenant_id, limit_id).await?;
    Ok(Json(serde_json::json!({ "success": true })))
}
