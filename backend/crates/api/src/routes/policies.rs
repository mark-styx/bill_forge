//! Natural-language policy composer routes (Invoice Processing module)

use crate::error::ApiResult;
use crate::extractors::InvoiceProcessingAccess;
use crate::routes::workflows::log_audit_or_record_gap;
use crate::services::policy_composer::{
    parse_policies, preview_against_history, ProposedRule,
};
use crate::state::AppState;
use axum::{extract::State, routing::post, Json, Router};
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

/// A parsed rule paired with its own 90-day preview and per-rule warnings.
#[derive(Debug, Serialize)]
pub struct ProposedRuleWithPreview {
    pub rule: ProposedRule,
    pub preview: PolicyPreviewResponse,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ComposeResponse {
    /// Authoritative list of parsed rules, each with its own preview.
    pub proposed_rules: Vec<ProposedRuleWithPreview>,
    /// Segments of the input that did not match any known pattern (surfaced to
    /// the admin so they can see what was ignored).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub unparseable_segments: Vec<String>,
    /// Backward-compat: the first parsed rule mirrored as a top-level single
    /// rule, for any caller still reading the legacy single-rule shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposed_rule: Option<ProposedRule>,
    /// Backward-compat: preview for the first parsed rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<PolicyPreviewResponse>,
    /// Backward-compat: warnings for the first parsed rule.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Clone)]
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
    /// Authoritative: list of rules to commit (compound policies).
    #[serde(default)]
    pub proposed_rules: Vec<ProposedRule>,
    /// Backward-compat: single-rule form. Used when callers send only one rule.
    #[serde(default)]
    pub proposed_rule: Option<ProposedRule>,
    pub original_text: String,
}

async fn compose_policy(
    State(state): State<AppState>,
    InvoiceProcessingAccess(_user, tenant): InvoiceProcessingAccess,
    Json(body): Json<ComposeRequest>,
) -> ApiResult<Json<ComposeResponse>> {
    let text = body.text.trim().to_string();
    if text.is_empty() {
        return Err(
            billforge_core::Error::Validation("Policy text cannot be empty".to_string()).into(),
        );
    }

    let parsed = parse_policies(&text).map_err(|e| {
        billforge_core::Error::Validation(format!("Could not parse policy: {}", e.message))
    })?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let mut proposed_rules: Vec<ProposedRuleWithPreview> = Vec::with_capacity(parsed.rules.len());
    for rule in parsed.rules {
        let preview = preview_against_history(&tenant.tenant_id, &rule, &pool).await?;
        let mut warnings = Vec::new();
        if preview.matched_count == 0 {
            warnings.push("This rule would not match any invoices from the last 90 days.".to_string());
        }
        let preview_response = PolicyPreviewResponse {
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
        };
        proposed_rules.push(ProposedRuleWithPreview {
            rule,
            preview: preview_response,
            warnings,
        });
    }

    // Backward-compat: mirror the first rule into the legacy single-rule fields.
    let (proposed_rule, preview, warnings) = match proposed_rules.first() {
        Some(first) => (
            Some(first.rule.clone()),
            Some(first.preview.clone()),
            Some(first.warnings.clone()),
        ),
        None => (None, None, None),
    };

    Ok(Json(ComposeResponse {
        proposed_rules,
        unparseable_segments: parsed.unparseable_segments,
        proposed_rule,
        preview,
        warnings,
    }))
}

async fn commit_policy(
    State(state): State<AppState>,
    InvoiceProcessingAccess(user, tenant): InvoiceProcessingAccess,
    Json(body): Json<CommitRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Normalize the request into a list of rules. Prefer the explicit list;
    // fall back to the legacy single-rule field for backward compatibility.
    let mut rules: Vec<ProposedRule> = body.proposed_rules.clone();
    if rules.is_empty() {
        if let Some(single) = body.proposed_rule.clone() {
            rules.push(single);
        }
    }

    if rules.is_empty() {
        return Err(billforge_core::Error::Validation(
            "No proposed rules supplied for commit".to_string(),
        )
        .into());
    }

    // Re-validate the original NL text through the parser.
    if parse_policies(&body.original_text).is_err() {
        return Err(billforge_core::Error::Validation(format!(
            "Rule validation failed: original text did not parse"
        ))
        .into());
    }

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::WorkflowRepositoryImpl::new(pool.clone());

    let mut rule_ids: Vec<String> = Vec::with_capacity(rules.len());
    for (clause_index, proposed) in rules.iter().enumerate() {
        let input = billforge_core::domain::CreateWorkflowRuleInput {
            name: proposed.name.clone(),
            description: Some(proposed.description.clone()),
            priority: proposed.priority,
            rule_type: proposed.to_rule_type(),
            conditions: proposed.to_conditions(),
            actions: proposed.to_actions(),
        };

        let rule =
            billforge_core::traits::WorkflowRuleRepository::create(&repo, &tenant.tenant_id, input)
                .await?;
        rule_ids.push(rule.id.to_string());

        // One audit entry per committed rule, carrying the original NL text,
        // the matching guardrail_kind, and the clause index.
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
            "guardrail_kind": proposed.guardrail_kind,
            "rule_id": rule.id.to_string(),
            "original_clause_index": clause_index,
        }));
        log_audit_or_record_gap(&pool, audit_entry).await;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "rule_ids": rule_ids.clone(),
        // Legacy single-rule field preserved for any existing caller.
        "rule_id": rule_ids.first().cloned(),
    })))
}
