//! Winston AI action proposal creation service.

use std::sync::Arc;

use billforge_core::{Error, Module, Result, Role, TenantContext, UserContext};
use billforge_db::repositories::{
    AiActionProposalRecord, AiActionProposalRepositoryImpl, AiActionProposalRisk,
    AiActionProposalStatus, CreateAiActionProposalInput,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::tools::{AiToolPermission, AiToolRiskLevel, ToolRegistry};

/// Input for creating a pending Winston action proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWinstonProposalInput {
    pub conversation_id: Uuid,
    pub tool_name: String,
    pub payload: serde_json::Value,
    pub risk: AiActionProposalRisk,
    pub permission: String,
}

/// Service for creating Winston action proposals after module access checks.
pub struct WinstonProposalService {
    repository: AiActionProposalRepositoryImpl,
}

impl WinstonProposalService {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            repository: AiActionProposalRepositoryImpl::new(pool),
        }
    }

    pub async fn create_pending_proposal(
        &self,
        tenant: &TenantContext,
        user: &UserContext,
        input: CreateWinstonProposalInput,
    ) -> Result<AiActionProposalRecord> {
        if !tenant.has_module(Module::AiAssistant) {
            return Err(Error::ModuleNotAvailable(
                Module::AiAssistant.display_name().to_string(),
            ));
        }

        let tool_definition =
            ToolRegistry::get_tool_definition(&input.tool_name).ok_or_else(|| {
                Error::Validation(format!("Unknown Winston tool: {}", input.tool_name))
            })?;
        let required_permission =
            permission_to_persisted_string(tool_definition.required_permission);
        let required_risk = risk_level_to_proposal_risk(tool_definition.risk_level);

        if input.permission != required_permission {
            return Err(Error::Validation(format!(
                "Tool metadata permission mismatch for {}",
                input.tool_name
            )));
        }

        if input.risk != required_risk {
            return Err(Error::Validation(format!(
                "Tool metadata risk mismatch for {}",
                input.tool_name
            )));
        }

        if !user_roles_allow_proposal_risk(user, required_risk) {
            return Err(Error::Forbidden(format!(
                "User may not request {} risk Winston proposals",
                required_risk.as_str()
            )));
        }

        if !user_roles_grant_tool_permission(user, tool_definition.required_permission) {
            return Err(Error::Forbidden(format!(
                "User may not request Winston tool permission {}",
                required_permission
            )));
        }

        if !user_roles_grant_permission_and_risk(
            user,
            tool_definition.required_permission,
            required_risk,
        ) {
            return Err(Error::Forbidden(
                "User role does not grant the requested Winston tool permission and risk"
                    .to_string(),
            ));
        }

        self.repository
            .create_proposal(
                &tenant.tenant_id,
                &user.user_id,
                CreateAiActionProposalInput {
                    conversation_id: input.conversation_id,
                    tool_name: input.tool_name,
                    payload: input.payload,
                    risk: input.risk,
                    permission: input.permission,
                },
            )
            .await
    }
}

/// Validate that a user may approve or reject a persisted Winston action proposal.
pub fn validate_action_proposal_decision(
    tenant: &TenantContext,
    user: &UserContext,
    proposal: &AiActionProposalRecord,
) -> Result<()> {
    if !tenant.has_module(Module::AiAssistant) {
        return Err(Error::ModuleNotAvailable(
            Module::AiAssistant.display_name().to_string(),
        ));
    }

    if proposal.tenant_id != *tenant.tenant_id.as_uuid() {
        return Err(Error::CrossTenantAccess);
    }

    if proposal.user_id != user.user_id.0 {
        return Err(Error::Forbidden(
            "User may not decide another user's Winston action proposal".to_string(),
        ));
    }

    if proposal.status != AiActionProposalStatus::Pending {
        return Err(Error::Conflict(format!(
            "Winston action proposal {} is not pending",
            proposal.id
        )));
    }

    let tool_definition =
        ToolRegistry::get_tool_definition(&proposal.tool_name).ok_or_else(|| {
            Error::Validation(format!("Unknown Winston tool: {}", proposal.tool_name))
        })?;
    let required_permission = permission_to_persisted_string(tool_definition.required_permission);
    let required_risk = risk_level_to_proposal_risk(tool_definition.risk_level);

    if proposal.permission != required_permission {
        return Err(Error::Validation(format!(
            "Tool metadata permission mismatch for {}",
            proposal.tool_name
        )));
    }

    if proposal.risk != required_risk {
        return Err(Error::Validation(format!(
            "Tool metadata risk mismatch for {}",
            proposal.tool_name
        )));
    }

    if !user_roles_allow_proposal_risk(user, required_risk) {
        return Err(Error::Forbidden(format!(
            "User may not decide {} risk Winston proposals",
            required_risk.as_str()
        )));
    }

    if !user_roles_grant_tool_permission(user, tool_definition.required_permission) {
        return Err(Error::Forbidden(format!(
            "User may not decide Winston tool permission {}",
            required_permission
        )));
    }

    if !user_roles_grant_permission_and_risk(
        user,
        tool_definition.required_permission,
        required_risk,
    ) {
        return Err(Error::Forbidden(
            "User role does not grant the requested Winston tool permission and risk".to_string(),
        ));
    }

    Ok(())
}

fn permission_to_persisted_string(permission: AiToolPermission) -> &'static str {
    match permission {
        AiToolPermission::InvoiceRead => "invoice.read",
        AiToolPermission::VendorRead => "vendor.read",
        AiToolPermission::ApprovalRead => "approval.read",
        AiToolPermission::ApprovalRespond => "approval.respond",
        AiToolPermission::TenantModuleRead => "tenant.module.read",
        AiToolPermission::ProductKnowledgeRead => "product.knowledge.read",
        AiToolPermission::WorkflowRead => "workflow.read",
        AiToolPermission::IssueRequest => "issue.request",
        AiToolPermission::AdminAnalyticsRead => "admin.analytics.read",
    }
}

fn risk_level_to_proposal_risk(risk_level: AiToolRiskLevel) -> AiActionProposalRisk {
    match risk_level {
        AiToolRiskLevel::Low => AiActionProposalRisk::Low,
        AiToolRiskLevel::Medium => AiActionProposalRisk::Medium,
        AiToolRiskLevel::High => AiActionProposalRisk::High,
    }
}

fn user_roles_grant_tool_permission(user: &UserContext, permission: AiToolPermission) -> bool {
    user.roles
        .iter()
        .any(|role| role_grants_tool_permission(*role, permission))
}

fn user_roles_allow_proposal_risk(user: &UserContext, risk: AiActionProposalRisk) -> bool {
    user.roles
        .iter()
        .any(|role| role_allows_proposal_risk(*role, risk))
}

fn user_roles_grant_permission_and_risk(
    user: &UserContext,
    permission: AiToolPermission,
    risk: AiActionProposalRisk,
) -> bool {
    user.roles.iter().any(|role| {
        role_grants_tool_permission(*role, permission) && role_allows_proposal_risk(*role, risk)
    })
}

fn role_grants_tool_permission(role: Role, permission: AiToolPermission) -> bool {
    match role {
        Role::TenantAdmin => true,
        Role::ApUser => matches!(
            permission,
            AiToolPermission::InvoiceRead
                | AiToolPermission::ApprovalRead
                | AiToolPermission::ApprovalRespond
                | AiToolPermission::WorkflowRead
                | AiToolPermission::IssueRequest
        ),
        Role::Approver => matches!(
            permission,
            AiToolPermission::InvoiceRead
                | AiToolPermission::ApprovalRead
                | AiToolPermission::ApprovalRespond
                | AiToolPermission::WorkflowRead
        ),
        Role::VendorManager => matches!(permission, AiToolPermission::VendorRead),
        Role::ReportViewer => matches!(
            permission,
            AiToolPermission::TenantModuleRead | AiToolPermission::ProductKnowledgeRead
        ),
        Role::Custom(_) => false,
    }
}

fn role_allows_proposal_risk(role: Role, risk: AiActionProposalRisk) -> bool {
    role_max_proposal_risk(role).is_some_and(|max_risk| risk_rank(risk) <= risk_rank(max_risk))
}

fn role_max_proposal_risk(role: Role) -> Option<AiActionProposalRisk> {
    match role {
        Role::TenantAdmin => Some(AiActionProposalRisk::High),
        Role::ApUser | Role::Approver | Role::VendorManager => Some(AiActionProposalRisk::Medium),
        Role::ReportViewer => Some(AiActionProposalRisk::Low),
        Role::Custom(_) => None,
    }
}

fn risk_rank(risk: AiActionProposalRisk) -> u8 {
    match risk {
        AiActionProposalRisk::Low => 0,
        AiActionProposalRisk::Medium => 1,
        AiActionProposalRisk::High => 2,
    }
}
