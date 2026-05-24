//! AI action proposal repository implementation

use billforge_core::{
    domain::{AuditAction, ResourceType},
    Error, Result, TenantId, UserId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
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
    Pending,
    Approved,
    Rejected,
    Executed,
    Failed,
}

impl AiActionProposalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiActionProposalStatus::Pending => "pending",
            AiActionProposalStatus::Approved => "approved",
            AiActionProposalStatus::Rejected => "rejected",
            AiActionProposalStatus::Executed => "executed",
            AiActionProposalStatus::Failed => "failed",
        }
    }

    fn from_db(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(AiActionProposalStatus::Pending),
            "approved" => Ok(AiActionProposalStatus::Approved),
            "rejected" => Ok(AiActionProposalStatus::Rejected),
            "executed" => Ok(AiActionProposalStatus::Executed),
            "failed" => Ok(AiActionProposalStatus::Failed),
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
    pub execution_error_code: Option<String>,
    pub execution_error_message: Option<String>,
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
    pub execution_error_code: Option<String>,
    pub execution_error_message: Option<String>,
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
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Database(format!("Failed to begin transaction: {}", e)))?;

        let row: AiActionProposalRow = sqlx::query_as::<_, AiActionProposalRow>(
            r#"INSERT INTO ai_action_proposals (
                    tenant_id, user_id, conversation_id, tool_name, payload, risk, permission
               ) VALUES ($1, $2, $3, $4, $5, $6, $7)
               RETURNING id, tenant_id, user_id, conversation_id, tool_name, payload,
                         risk, permission, status, execution_error_code,
                         execution_error_message, created_at, updated_at"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(input.conversation_id)
        .bind(&input.tool_name)
        .bind(&input.payload)
        .bind(input.risk.as_str())
        .bind(&input.permission)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to create action proposal: {}", e)))?;

        let proposal = row.into_record()?;
        Self::write_proposal_created_audit(&mut tx, &proposal).await?;

        tx.commit()
            .await
            .map_err(|e| Error::Database(format!("Failed to commit action proposal: {}", e)))?;

        Ok(proposal)
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
                      risk, permission, status, execution_error_code,
                      execution_error_message, created_at, updated_at
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
                      risk, permission, status, execution_error_code,
                      execution_error_message, created_at, updated_at
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

    /// List pending action proposals for a conversation, newest first.
    pub async fn list_pending_proposals_for_conversation(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        conversation_id: Uuid,
    ) -> Result<Vec<AiActionProposalRecord>> {
        let rows: Vec<AiActionProposalRow> = sqlx::query_as::<_, AiActionProposalRow>(
            r#"SELECT id, tenant_id, user_id, conversation_id, tool_name, payload,
                      risk, permission, status, execution_error_code,
                      execution_error_message, created_at, updated_at
               FROM ai_action_proposals
               WHERE tenant_id = $1
                 AND user_id = $2
                 AND conversation_id = $3
                 AND status = 'pending'
               ORDER BY created_at DESC"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(conversation_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list pending action proposals: {}", e)))?;

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
        input: UpdateAiActionProposalStatusInput,
    ) -> Result<AiActionProposalRecord> {
        self.update_proposal_status_with_required_current_status(
            tenant_id,
            user_id,
            proposal_id,
            input,
            None,
        )
        .await
    }

    /// Update action proposal status only when the row is still pending.
    pub async fn update_pending_proposal_status(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        proposal_id: Uuid,
        input: UpdateAiActionProposalStatusInput,
    ) -> Result<AiActionProposalRecord> {
        self.update_proposal_status_with_required_current_status(
            tenant_id,
            user_id,
            proposal_id,
            input,
            Some(AiActionProposalStatus::Pending),
        )
        .await
    }

    /// Approve an action proposal only when the row is still pending.
    pub async fn approve_pending_proposal(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        proposal_id: Uuid,
    ) -> Result<AiActionProposalRecord> {
        self.update_proposal_status_with_required_current_status(
            tenant_id,
            user_id,
            proposal_id,
            UpdateAiActionProposalStatusInput {
                status: AiActionProposalStatus::Approved,
                execution_error_code: None,
                execution_error_message: None,
            },
            Some(AiActionProposalStatus::Pending),
        )
        .await
    }

    async fn update_proposal_status_with_required_current_status(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        proposal_id: Uuid,
        input: UpdateAiActionProposalStatusInput,
        required_current_status: Option<AiActionProposalStatus>,
    ) -> Result<AiActionProposalRecord> {
        let status = input.status;
        let (execution_error_code, execution_error_message) =
            if status == AiActionProposalStatus::Failed {
                (
                    Some(input.execution_error_code.ok_or_else(|| {
                        Error::Validation(
                            "Failed action proposals require execution_error_code".to_string(),
                        )
                    })?),
                    Some(input.execution_error_message.ok_or_else(|| {
                        Error::Validation(
                            "Failed action proposals require execution_error_message".to_string(),
                        )
                    })?),
                )
            } else {
                (None, None)
            };

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Database(format!("Failed to begin transaction: {}", e)))?;

        let old_status: String = sqlx::query_scalar(
            r#"SELECT status
               FROM ai_action_proposals
               WHERE tenant_id = $1 AND user_id = $2 AND id = $3
               FOR UPDATE"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(proposal_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            Error::Database(format!(
                "Failed to load action proposal status for update: {}",
                e
            ))
        })?
        .ok_or_else(|| Error::NotFound {
            resource_type: "ai_action_proposal".to_string(),
            id: proposal_id.to_string(),
        })?;
        let old_status = AiActionProposalStatus::from_db(&old_status)?;

        if let Some(required_current_status) = required_current_status {
            if old_status != required_current_status {
                return Err(Error::Conflict(format!(
                    "AI action proposal {} is not {}",
                    proposal_id,
                    required_current_status.as_str()
                )));
            }
        }

        let row: AiActionProposalRow = sqlx::query_as::<_, AiActionProposalRow>(
            r#"UPDATE ai_action_proposals
               SET status = $4,
                   execution_error_code = $5,
                   execution_error_message = $6,
                   updated_at = NOW()
               WHERE tenant_id = $1 AND user_id = $2 AND id = $3
               RETURNING id, tenant_id, user_id, conversation_id, tool_name, payload,
                         risk, permission, status, execution_error_code,
                         execution_error_message, created_at, updated_at"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(proposal_id)
        .bind(status.as_str())
        .bind(execution_error_code.as_deref())
        .bind(execution_error_message.as_deref())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to update action proposal status: {}", e)))?;

        let proposal = row.into_record()?;
        Self::write_proposal_status_audit(&mut tx, &proposal, old_status).await?;

        tx.commit().await.map_err(|e| {
            Error::Database(format!(
                "Failed to commit action proposal status update: {}",
                e
            ))
        })?;

        Ok(proposal)
    }

    async fn write_proposal_created_audit(
        tx: &mut Transaction<'_, Postgres>,
        proposal: &AiActionProposalRecord,
    ) -> Result<()> {
        Self::insert_audit_log(
            tx,
            proposal,
            AuditAction::AiActionProposalCreated,
            "AI action proposal created",
            None,
            None,
            serde_json::json!({
                "conversation_id": proposal.conversation_id,
                "tool_name": &proposal.tool_name,
                "risk": proposal.risk.as_str(),
                "permission": &proposal.permission,
                "status": proposal.status.as_str(),
            }),
        )
        .await
    }

    async fn write_proposal_status_audit(
        tx: &mut Transaction<'_, Postgres>,
        proposal: &AiActionProposalRecord,
        old_status: AiActionProposalStatus,
    ) -> Result<()> {
        let Some(action) = Self::audit_action_for_status(proposal.status) else {
            return Ok(());
        };

        let metadata = if proposal.status == AiActionProposalStatus::Failed {
            serde_json::json!({
                "execution_error_code": proposal.execution_error_code.as_deref(),
                "execution_error_message": proposal.execution_error_message.as_deref(),
            })
        } else {
            serde_json::Value::Null
        };

        Self::insert_audit_log(
            tx,
            proposal,
            action,
            action.label(),
            Some(serde_json::json!(old_status.as_str())),
            Some(serde_json::json!(proposal.status.as_str())),
            metadata,
        )
        .await
    }

    fn audit_action_for_status(status: AiActionProposalStatus) -> Option<AuditAction> {
        match status {
            AiActionProposalStatus::Pending => None,
            AiActionProposalStatus::Approved => Some(AuditAction::AiActionProposalApproved),
            AiActionProposalStatus::Rejected => Some(AuditAction::AiActionProposalRejected),
            AiActionProposalStatus::Executed => Some(AuditAction::AiActionProposalExecuted),
            AiActionProposalStatus::Failed => Some(AuditAction::AiActionProposalFailed),
        }
    }

    async fn insert_audit_log(
        tx: &mut Transaction<'_, Postgres>,
        proposal: &AiActionProposalRecord,
        action: AuditAction,
        description: &str,
        old_value: Option<serde_json::Value>,
        new_value: Option<serde_json::Value>,
        metadata: serde_json::Value,
    ) -> Result<()> {
        let action_str = serde_json::to_string(&action)
            .map_err(|e| Error::Database(format!("Failed to serialize audit action: {}", e)))?
            .trim_matches('"')
            .to_string();
        let resource_type_str = serde_json::to_string(&ResourceType::AiActionProposal)
            .map_err(|e| {
                Error::Database(format!("Failed to serialize audit resource type: {}", e))
            })?
            .trim_matches('"')
            .to_string();

        sqlx::query(
            r#"INSERT INTO audit_log (
                id, tenant_id, user_id, action, resource_type, resource_id,
                changes, ip_address, user_agent, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, NULL, $8)"#,
        )
        .bind(Uuid::new_v4())
        .bind(proposal.tenant_id)
        .bind(proposal.user_id)
        .bind(action_str)
        .bind(resource_type_str)
        .bind(proposal.id.to_string())
        .bind(serde_json::json!({
            "description": description,
            "old_value": old_value,
            "new_value": new_value,
            "user_email": null,
            "metadata": metadata,
        }))
        .bind(Utc::now())
        .execute(&mut **tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to insert proposal audit entry: {}", e)))?;

        Ok(())
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
    execution_error_code: Option<String>,
    execution_error_message: Option<String>,
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
            execution_error_code: self.execution_error_code,
            execution_error_message: self.execution_error_message,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
