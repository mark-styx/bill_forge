//! AI action proposal repository implementation

use billforge_core::{Error, Result, TenantId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Risk level for an AI action proposal, matching the DB CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiActionProposalRisk {
    Low,
    Medium,
    High,
}

impl AiActionProposalRisk {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiActionProposalRisk::Low => "low",
            AiActionProposalRisk::Medium => "medium",
            AiActionProposalRisk::High => "high",
        }
    }

    fn from_db(value: &str) -> Result<Self> {
        match value {
            "low" => Ok(AiActionProposalRisk::Low),
            "medium" => Ok(AiActionProposalRisk::Medium),
            "high" => Ok(AiActionProposalRisk::High),
            other => Err(Error::Database(format!(
                "Invalid ai_action_proposals.risk value: {}",
                other
            ))),
        }
    }
}

/// Status for an AI action proposal, matching the DB CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiActionProposalStatus {
    ApprovalRequired,
    Approved,
    Rejected,
    Executed,
    Cancelled,
    Expired,
}

impl AiActionProposalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiActionProposalStatus::ApprovalRequired => "approval_required",
            AiActionProposalStatus::Approved => "approved",
            AiActionProposalStatus::Rejected => "rejected",
            AiActionProposalStatus::Executed => "executed",
            AiActionProposalStatus::Cancelled => "cancelled",
            AiActionProposalStatus::Expired => "expired",
        }
    }

    fn from_db(value: &str) -> Result<Self> {
        match value {
            "approval_required" => Ok(AiActionProposalStatus::ApprovalRequired),
            "approved" => Ok(AiActionProposalStatus::Approved),
            "rejected" => Ok(AiActionProposalStatus::Rejected),
            "executed" => Ok(AiActionProposalStatus::Executed),
            "cancelled" => Ok(AiActionProposalStatus::Cancelled),
            "expired" => Ok(AiActionProposalStatus::Expired),
            other => Err(Error::Database(format!(
                "Invalid ai_action_proposals.status value: {}",
                other
            ))),
        }
    }
}

/// Input for creating an AI action proposal.
#[derive(Debug, Clone)]
pub struct CreateAiActionProposalInput {
    pub conversation_id: Uuid,
    pub tool_name: String,
    pub payload: serde_json::Value,
    pub risk: AiActionProposalRisk,
    pub permission: String,
}

/// Input for updating an AI action proposal status.
#[derive(Debug, Clone)]
pub struct UpdateAiActionProposalStatusInput {
    pub status: AiActionProposalStatus,
}

/// A persisted AI action proposal row.
#[derive(Debug, Clone)]
pub struct AiActionProposalRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub conversation_id: Uuid,
    pub tool_name: String,
    pub payload: serde_json::Value,
    pub risk: AiActionProposalRisk,
    pub permission: String,
    pub status: AiActionProposalStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// PostgreSQL implementation of the AI action proposal repository.
pub struct AiActionProposalRepositoryImpl {
    pool: Arc<PgPool>,
}

impl AiActionProposalRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Create a new action proposal scoped to the given tenant, user, and conversation.
    pub async fn create_proposal(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        input: CreateAiActionProposalInput,
    ) -> Result<AiActionProposalRecord> {
        let row: AiActionProposalRow = sqlx::query_as::<_, AiActionProposalRow>(
            r#"INSERT INTO ai_action_proposals (
                    tenant_id, user_id, conversation_id, tool_name, payload, risk, permission
               ) VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id, tenant_id, user_id, conversation_id, tool_name, payload,
                         risk, permission, status, created_at, updated_at"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(input.conversation_id)
        .bind(&input.tool_name)
        .bind(&input.payload)
        .bind(input.risk.as_str())
        .bind(&input.permission)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create action proposal: {}", e)))?;

        row.into_record()
    }

    /// Load an action proposal scoped by tenant and user.
    pub async fn get_proposal(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        proposal_id: Uuid,
    ) -> Result<Option<AiActionProposalRecord>> {
        let row: Option<AiActionProposalRow> = sqlx::query_as::<_, AiActionProposalRow>(
            r#"SELECT id, tenant_id, user_id, conversation_id, tool_name, payload,
                      risk, permission, status, created_at, updated_at
               FROM ai_action_proposals
               WHERE id = $1 AND tenant_id = $2 AND user_id = $3"#,
        )
        .bind(proposal_id)
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get action proposal: {}", e)))?;

        row.map(AiActionProposalRow::into_record).transpose()
    }

    /// List all action proposals for a conversation, newest first.
    pub async fn list_proposals_for_conversation(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        conversation_id: Uuid,
    ) -> Result<Vec<AiActionProposalRecord>> {
        let rows: Vec<AiActionProposalRow> = sqlx::query_as::<_, AiActionProposalRow>(
            r#"SELECT id, tenant_id, user_id, conversation_id, tool_name, payload,
                      risk, permission, status, created_at, updated_at
               FROM ai_action_proposals
               WHERE tenant_id = $1 AND user_id = $2 AND conversation_id = $3
               ORDER BY created_at DESC"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(conversation_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list action proposals: {}", e)))?;

        rows.into_iter()
            .map(AiActionProposalRow::into_record)
            .collect()
    }

    /// Update action proposal status, scoped by tenant and user.
    pub async fn update_proposal_status(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        proposal_id: Uuid,
        status: AiActionProposalStatus,
    ) -> Result<AiActionProposalRecord> {
        let row: AiActionProposalRow = sqlx::query_as::<_, AiActionProposalRow>(
            r#"UPDATE ai_action_proposals
               SET status = $4, updated_at = NOW()
               WHERE tenant_id = $1 AND user_id = $2 AND id = $3
               RETURNING id, tenant_id, user_id, conversation_id, tool_name, payload,
                         risk, permission, status, created_at, updated_at"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(proposal_id)
        .bind(status.as_str())
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::RowNotFound = e {
                Error::NotFound {
                    resource_type: "ai_action_proposal".to_string(),
                    id: proposal_id.to_string(),
                }
            } else {
                Error::Database(format!("Failed to update action proposal status: {}", e))
            }
        })?;

        row.into_record()
    }
}

// ---------------------------------------------------------------------------
// Internal row mapping helpers
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct AiActionProposalRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    conversation_id: Uuid,
    tool_name: String,
    payload: serde_json::Value,
    risk: String,
    permission: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AiActionProposalRow {
    fn into_record(self) -> Result<AiActionProposalRecord> {
        Ok(AiActionProposalRecord {
            id: self.id,
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            conversation_id: self.conversation_id,
            tool_name: self.tool_name,
            payload: self.payload,
            risk: AiActionProposalRisk::from_db(&self.risk)?,
            permission: self.permission,
            status: AiActionProposalStatus::from_db(&self.status)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
