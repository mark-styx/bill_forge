//! AI conversation and message repository implementation

use billforge_core::{Error, Result, TenantId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

/// Role of a participant in a conversation, matching the DB CHECK constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMessageRole {
    System,
    User,
    Assistant,
}

impl AiMessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiMessageRole::System => "system",
            AiMessageRole::User => "user",
            AiMessageRole::Assistant => "assistant",
        }
    }
}

/// Token usage telemetry for a single message.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AiMessageUsage {
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
}

/// Input for appending a message to a conversation.
#[derive(Debug, Clone)]
pub struct AppendAiMessageInput {
    pub role: AiMessageRole,
    pub content: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub model_route: Option<String>,
    pub finish_reason: Option<String>,
    pub provider_request_id: Option<String>,
    pub latency_ms: Option<i64>,
    pub usage: Option<AiMessageUsage>,
    pub metadata: serde_json::Value,
}

/// A persisted AI conversation row.
#[derive(Debug, Clone)]
pub struct AiConversationRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub title: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A persisted AI message row.
#[derive(Debug, Clone)]
pub struct AiMessageRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub conversation_id: Uuid,
    pub role: String,
    pub content: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub model_route: Option<String>,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
    pub finish_reason: Option<String>,
    pub provider_request_id: Option<String>,
    pub latency_ms: Option<i64>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// PostgreSQL implementation of the AI conversation repository.
pub struct AiConversationRepositoryImpl {
    pool: Arc<PgPool>,
}

impl AiConversationRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Create a new conversation scoped to the given tenant and user.
    pub async fn create_conversation(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        title: Option<&str>,
        metadata: serde_json::Value,
    ) -> Result<AiConversationRecord> {
        let row: ConversationRow = sqlx::query_as::<_, ConversationRow>(
            r#"INSERT INTO ai_conversations (tenant_id, user_id, title, metadata)
               VALUES ($1, $2, $3, $4)
               RETURNING id, tenant_id, user_id, title, metadata, created_at, updated_at"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(title)
        .bind(&metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create conversation: {}", e)))?;

        Ok(row.into_record())
    }

    /// Append a message to an existing conversation.
    ///
    /// Updates `ai_conversations.updated_at` scoped by tenant/user/conversation.
    /// Returns `Error::NotFound` if the conversation does not exist or does not
    /// belong to the given tenant/user.
    pub async fn append_message(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        conversation_id: Uuid,
        input: AppendAiMessageInput,
    ) -> Result<AiMessageRecord> {
        // Touch the conversation's updated_at, scoped by tenant/user/conversation.
        let touched = sqlx::query(
            r#"UPDATE ai_conversations
               SET updated_at = NOW()
               WHERE id = $1 AND tenant_id = $2 AND user_id = $3"#,
        )
        .bind(conversation_id)
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update conversation timestamp: {}", e)))?;

        if touched.rows_affected() == 0 {
            return Err(Error::NotFound {
                resource_type: "ai_conversation".to_string(),
                id: conversation_id.to_string(),
            });
        }

        let usage = input.usage.unwrap_or_default();

        let row: MessageRow = sqlx::query_as::<_, MessageRow>(
            r#"INSERT INTO ai_messages (
                    tenant_id, user_id, conversation_id, role, content,
                    provider, model, model_route,
                    prompt_tokens, completion_tokens, total_tokens,
                    finish_reason, provider_request_id, latency_ms, metadata
               ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
               RETURNING id, tenant_id, user_id, conversation_id, role, content,
                         provider, model, model_route,
                         prompt_tokens, completion_tokens, total_tokens,
                         finish_reason, provider_request_id, latency_ms, metadata, created_at"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(user_id.0)
        .bind(conversation_id)
        .bind(input.role.as_str())
        .bind(&input.content)
        .bind(&input.provider)
        .bind(&input.model)
        .bind(&input.model_route)
        .bind(usage.prompt_tokens)
        .bind(usage.completion_tokens)
        .bind(usage.total_tokens)
        .bind(&input.finish_reason)
        .bind(&input.provider_request_id)
        .bind(input.latency_ms)
        .bind(&input.metadata)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to append message: {}", e)))?;

        Ok(row.into_record())
    }
}

// ---------------------------------------------------------------------------
// Internal row mapping helpers
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    title: Option<String>,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ConversationRow {
    fn into_record(self) -> AiConversationRecord {
        AiConversationRecord {
            id: self.id,
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            title: self.title,
            metadata: self.metadata,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    conversation_id: Uuid,
    role: String,
    content: String,
    provider: Option<String>,
    model: Option<String>,
    model_route: Option<String>,
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
    total_tokens: Option<i32>,
    finish_reason: Option<String>,
    provider_request_id: Option<String>,
    latency_ms: Option<i64>,
    metadata: serde_json::Value,
    created_at: DateTime<Utc>,
}

impl MessageRow {
    fn into_record(self) -> AiMessageRecord {
        AiMessageRecord {
            id: self.id,
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            conversation_id: self.conversation_id,
            role: self.role,
            content: self.content,
            provider: self.provider,
            model: self.model,
            model_route: self.model_route,
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            finish_reason: self.finish_reason,
            provider_request_id: self.provider_request_id,
            latency_ms: self.latency_ms,
            metadata: self.metadata,
            created_at: self.created_at,
        }
    }
}
