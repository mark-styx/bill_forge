//! Approval chain routes — policies, multi-level approvals, escalation, delegation

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
    Json, Router,
};
use billforge_approval_engine::{
    ActiveApprovalChain, ApprovalActivity, ApprovalChainDetail, ApprovalChainStep,
    ApprovalEngine, ApprovalPolicy, CreatePolicyInput, PendingApprovalSummary,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Policies
        .route("/policies", get(list_policies))
        .route("/policies", post(create_policy))
        .route("/policies/:id", get(get_policy))
        .route("/policies/:id", put(update_policy))
        .route("/policies/:id", delete(delete_policy))
        // Chains (active approval processes)
        .route("/chains", get(list_chains))
        .route("/chains/:id", get(get_chain_detail))
        .route("/chains/:id/recall", post(recall_chain))
        // Steps (individual approval actions)
        .route("/steps/:id/approve", post(approve_step))
        .route("/steps/:id/reject", post(reject_step))
        .route("/steps/:id/delegate", post(delegate_step))
        // My pending approvals
        .route("/pending", get(my_pending_approvals))
        // Submit invoice for approval
        .route("/submit/:invoice_id", post(submit_for_approval))
        // Escalation (admin: process overdue escalations)
        .route("/escalate", post(escalate_overdue))
        // Activity log for an invoice
        .route("/activity/:invoice_id", get(get_activity))
}

// ──────────────────────────── Request / Response Types ────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SubmitForApprovalInput {
    pub amount_cents: i64,
    pub department: Option<String>,
    pub vendor_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionInput {
    pub comments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DelegateStepInput {
    pub delegate_to: Uuid,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListChainsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct EscalationResult {
    pub escalated_count: usize,
    pub steps: Vec<ApprovalChainStep>,
}

// ──────────────────────────── Policy Handlers ────────────────────────────

/// List all approval policies for the tenant
async fn list_policies(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<Vec<ApprovalPolicy>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let policies = engine
        .policy_service()
        .list_policies(&tenant_uuid)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(policies))
}

/// Create a new approval policy
async fn create_policy(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(input): Json<CreatePolicyInput>,
) -> ApiResult<Json<ApprovalPolicy>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let policy = engine
        .policy_service()
        .create_policy(&tenant_uuid, input)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(policy))
}

/// Get a single approval policy by ID
async fn get_policy(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<ApprovalPolicy>> {
    let policy_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid policy ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let policy = engine
        .policy_service()
        .get_policy(&tenant_uuid, &policy_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ApprovalPolicy".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(policy))
}

/// Update an approval policy (partial update via JSON)
async fn update_policy(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
    Json(input): Json<serde_json::Value>,
) -> ApiResult<Json<ApprovalPolicy>> {
    let policy_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid policy ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let policy = engine
        .policy_service()
        .update_policy(&tenant_uuid, &policy_id, input)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(policy))
}

/// Delete an approval policy
async fn delete_policy(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let policy_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid policy ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    engine
        .policy_service()
        .delete_policy(&tenant_uuid, &policy_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ──────────────────────────── Chain Handlers ────────────────────────────

/// List active approval chains for the tenant
async fn list_chains(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Query(query): Query<ListChainsQuery>,
) -> ApiResult<Json<Vec<ActiveApprovalChain>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let limit = query.limit.unwrap_or(25);
    let offset = query.offset.unwrap_or(0);

    let status_clause = query
        .status
        .as_deref()
        .map(|s| format!("AND status = '{}'", s.replace('\'', "''")))
        .unwrap_or_default();

    let sql = format!(
        r#"
        SELECT id, tenant_id, invoice_id, policy_id, status,
               current_level, total_levels, final_decision,
               final_decided_by, final_decided_at, escalation_count,
               last_escalated_at, initiated_by, initiated_at,
               completed_at, created_at, updated_at
        FROM active_approval_chains
        WHERE tenant_id = $1 {}
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        status_clause
    );

    let rows = sqlx::query(&sql)
        .bind(tenant_uuid)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    use sqlx::Row;
    let chains: Vec<ActiveApprovalChain> = rows
        .iter()
        .map(|row| ActiveApprovalChain {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            invoice_id: row.get("invoice_id"),
            policy_id: row.get("policy_id"),
            status: row.get("status"),
            current_level: row.get("current_level"),
            total_levels: row.get("total_levels"),
            final_decision: row.get("final_decision"),
            final_decided_by: row.get("final_decided_by"),
            final_decided_at: row.get("final_decided_at"),
            escalation_count: row.get("escalation_count"),
            last_escalated_at: row.get("last_escalated_at"),
            initiated_by: row.get("initiated_by"),
            initiated_at: row.get("initiated_at"),
            completed_at: row.get("completed_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();

    Ok(Json(chains))
}

/// Get full chain detail (chain + policy + steps + activity)
async fn get_chain_detail(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<ApprovalChainDetail>> {
    let chain_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid chain ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let detail = engine
        .get_chain_detail(&tenant_uuid, &chain_id)
        .await
        .map_err(|e| billforge_core::Error::from(e))?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "ApprovalChain".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(detail))
}

/// Recall (cancel) an approval chain — only the initiator can recall
async fn recall_chain(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Json<ActiveApprovalChain>> {
    let chain_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid chain ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();

    let chain = engine
        .recall_chain(&tenant_uuid, &chain_id, &user_uuid)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(chain))
}

// ──────────────────────────── Step Handlers ────────────────────────────

/// Approve a step
async fn approve_step(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
    Json(input): Json<DecisionInput>,
) -> ApiResult<Json<ApprovalChainStep>> {
    let step_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid step ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();

    let step = engine
        .approve_step(
            &tenant_uuid,
            &step_id,
            &user_uuid,
            input.comments.as_deref(),
        )
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(step))
}

/// Reject a step — rejects the entire chain
async fn reject_step(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
    Json(input): Json<DecisionInput>,
) -> ApiResult<Json<ApprovalChainStep>> {
    let step_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid step ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();

    let step = engine
        .reject_step(
            &tenant_uuid,
            &step_id,
            &user_uuid,
            input.comments.as_deref(),
        )
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(step))
}

/// Delegate a step to another user
async fn delegate_step(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(id): Path<String>,
    Json(input): Json<DelegateStepInput>,
) -> ApiResult<Json<ApprovalChainStep>> {
    let step_id = Uuid::parse_str(&id)
        .map_err(|_| billforge_core::Error::Validation("Invalid step ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();

    let step = engine
        .delegate_step(
            &tenant_uuid,
            &step_id,
            &user_uuid,
            &input.delegate_to,
            input.reason.as_deref(),
        )
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(step))
}

// ──────────────────────────── User-Facing Handlers ────────────────────────────

/// Get pending approvals for the current user
async fn my_pending_approvals(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<Vec<PendingApprovalSummary>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();

    let pending = engine
        .get_pending_for_user(&tenant_uuid, &user_uuid)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(pending))
}

/// Submit an invoice for approval
async fn submit_for_approval(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(invoice_id): Path<String>,
    Json(input): Json<SubmitForApprovalInput>,
) -> ApiResult<Json<ActiveApprovalChain>> {
    let invoice_uuid = Uuid::parse_str(&invoice_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();

    let chain = engine
        .submit_for_approval(
            &tenant_uuid,
            &invoice_uuid,
            input.amount_cents,
            input.department.as_deref(),
            input.vendor_id.as_ref(),
            &user_uuid,
        )
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    Ok(Json(chain))
}

/// Process overdue escalations (admin endpoint)
async fn escalate_overdue(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<EscalationResult>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let engine = ApprovalEngine::new((*pool).clone());
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let steps = engine
        .escalate_overdue(&tenant_uuid)
        .await
        .map_err(|e| billforge_core::Error::from(e))?;

    let escalated_count = steps.len();

    Ok(Json(EscalationResult {
        escalated_count,
        steps,
    }))
}

/// Get activity log for an invoice's approval chain
async fn get_activity(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Path(invoice_id): Path<String>,
) -> ApiResult<Json<Vec<ApprovalActivity>>> {
    let invoice_uuid = Uuid::parse_str(&invoice_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid invoice ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    let rows = sqlx::query(
        r#"
        SELECT a.id, a.tenant_id, a.chain_id, a.step_id, a.invoice_id,
               a.action, a.actor_id, a.actor_role, a.comments, a.metadata,
               a.ip_address, a.created_at
        FROM approval_activity_log a
        WHERE a.tenant_id = $1 AND a.invoice_id = $2
        ORDER BY a.created_at ASC
        "#,
    )
    .bind(tenant_uuid)
    .bind(invoice_uuid)
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(e.to_string()))?;

    use sqlx::Row;
    let activity: Vec<ApprovalActivity> = rows
        .iter()
        .map(|row| ApprovalActivity {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            chain_id: row.get("chain_id"),
            step_id: row.get("step_id"),
            invoice_id: row.get("invoice_id"),
            action: row.get("action"),
            actor_id: row.get("actor_id"),
            actor_role: row.get("actor_role"),
            comments: row.get("comments"),
            metadata: row.get("metadata"),
            ip_address: row.get("ip_address"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok(Json(activity))
}
