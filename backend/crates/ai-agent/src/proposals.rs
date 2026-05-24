//! Winston AI action proposal creation service.

use std::sync::Arc;

use billforge_core::{Error, Module, Result, TenantContext, UserContext};
use billforge_db::repositories::{
    AiActionProposalRecord, AiActionProposalRepositoryImpl, AiActionProposalRisk,
    CreateAiActionProposalInput,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

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
