//! Natural-language policy composer routes (Invoice Processing module)

use crate::error::ApiResult;
use crate::extractors::InvoiceProcessingAccess;
use crate::routes::workflows::log_audit_or_record_gap;
use crate::services::policy_composer::{parse_policy, preview_against_history, ProposedRule};
use crate::state::AppState;
use axum::{
    extract::State,
    routing::post,
    Json, Router,
};
use billforge_core::domain::{AuditAction, AuditEntry, ResourceType};
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/compose", post(compose_policy))
        .route("/commit", post(commit_policy))
}

#[derive(Debug, Deserialize)]
pub struct ComposeRequest {
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct ComposeResponse {
    pub proposed_rule: ProposedRule,
    pub preview: PolicyPreviewResponse,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unparseable_segments: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PolicyPreviewResponse {
    pub matched_count: usize,
    pub total_invoices: usize,
    pub sample_invoices: Vec<InvoiceSummary>,
    pub projected_action_breakdown: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct InvoiceSummary {
    pub id: String,
    pub invoice_number: Option<String>,
    pub vendor_name: Option<String>,
    pub total_amount_cents: Option<i64>,
    pub processing_status: Option<String>,
    pub invoice_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CommitRequest {
    pub proposed_rule: ProposedRule,
    pub original_text: String,
}

async fn compose_policy(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Json(body): Json<ComposeRequest>,
) -> ApiResult<Json<ComposeResponse>> {
    let text = body.text.trim().to_string();
    if text.is_empty() {
        return Err(billforge_core::Error::Validation(
            "Policy text cannot be empty".to_string(),
        )
        .into());
    }

    let proposed_rule = parse_policy(&text).map_err(|e| {
        billforge_core::Error::Validation(format!("Could not parse policy: {}", e.message))
    })?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let preview = preview_against_history(&tenant.tenant_id, &proposed_rule, &pool).await?;

    let mut warnings = Vec::new();
    if preview.matched_count == 0 {
        warnings.push(
            "This rule would not match any invoices from the last 90 days.".to_string(),
        );
    }

    Ok(Json(ComposeResponse {
        proposed_rule,
        preview: PolicyPreviewResponse {
            matched_count: preview.matched_count,
            total_invoices: preview.total_invoices,
            sample_invoices: preview
                .sample_invoices
                .into_iter()
                .map(|inv| InvoiceSummary {
                    id: inv.id,
                    invoice_number: inv.invoice_number,
                    vendor_name: inv.vendor_name,
                    total_amount_cents: inv.total_amount_cents,
                    processing_status: inv.processing_status,
                    invoice_date: inv.invoice_date,
                })
                .collect(),
            projected_action_breakdown: preview.projected_action_breakdown,
        },
        warnings,
        unparseable_segments: vec![],
    }))
}

async fn commit_policy(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(body): Json<CommitRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Re-validate the rule through the parser
    let _proposed = parse_policy(&body.original_text).map_err(|e| {
        billforge_core::Error::Validation(format!("Rule validation failed: {}", e.message))
    })?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    let input = billforge_core::domain::CreateWorkflowRuleInput {
        name: body.proposed_rule.name.clone(),
        description: Some(body.proposed_rule.description.clone()),
        priority: body.proposed_rule.priority,
        rule_type: body.proposed_rule.to_rule_type(),
        conditions: body.proposed_rule.to_conditions(),
        actions: body.proposed_rule.to_actions(),
    };

    let rule = billforge_core::traits::WorkflowRuleRepository::create(
        &repo,
        &tenant.tenant_id,
        input,
    )
    .await?;

    // Audit log with original NL text
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::WorkflowRule,
        rule.id.to_string(),
        "Composed policy from natural language",
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "original_text": body.original_text,
        "guardrail_kind": body.proposed_rule.guardrail_kind,
        "rule_id": rule.id.to_string(),
    }));
    log_audit_or_record_gap(&pool, audit_entry).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "rule_id": rule.id.to_string(),
    })))
}
