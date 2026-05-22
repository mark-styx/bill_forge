use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Conversation message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Single message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Conversation with message history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub tenant_id: String,
    pub user_id: Uuid,
    pub messages: Vec<Message>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to send a message to Winston AI
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub conversation_id: Option<Uuid>,
}

/// Response from Winston AI
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub conversation_id: Uuid,
    pub message: Message,
}

/// Context injected into the AI agent
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub tenant_id: String,
    pub user_id: Uuid,
    pub user_role: String,
    pub permissions: Vec<String>,
}

/// Tool result from agent
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub tool_name: String,
    pub result: String,
    pub success: bool,
}

// ---------------------------------------------------------------------------
// Provider-neutral chat models
//
// These types are designed to be consumed by future provider adapters (OpenAI,
// GLM proxy, etc.) without depending on any specific SDK. They sit alongside
// the existing Winston-facing types above so current runtime behavior is
// unchanged.
// ---------------------------------------------------------------------------

/// Role of a message in a provider-neutral chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderMessageRole {
    System,
    User,
    Assistant,
}

/// A single message exchanged with an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderChatMessage {
    pub role: ProviderMessageRole,
    pub content: String,
}

/// A provider-neutral chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderChatRequest {
    /// Model identifier (e.g. "gpt-4o", "glm-4").
    pub model: String,
    /// Ordered list of conversation messages.
    pub messages: Vec<ProviderChatMessage>,
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

/// Token usage metadata returned by a provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProviderChatUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
}

/// A provider-neutral chat completion response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderChatResponse {
    /// The generated message.
    pub message: ProviderChatMessage,
    /// Why generation stopped, if reported by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Token usage metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ProviderChatUsage>,
    /// Opaque request id returned by the provider, useful for debugging.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
}

/// Category of a provider error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderChatErrorKind {
    /// The request was malformed or missing required fields.
    InvalidRequest,
    /// Authentication or authorization failure.
    Authentication,
    /// Rate limit exceeded; the caller should back off.
    RateLimit,
    /// The provider server encountered an internal error.
    Server,
    /// The requested model is not available.
    ModelNotFound,
    /// Context length exceeded.
    ContextLength,
    /// Any other error not covered above.
    Unknown,
}

/// A structured, serializable provider error.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderChatError {
    pub kind: ProviderChatErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_code: Option<String>,
    /// Whether the caller should retry the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}
