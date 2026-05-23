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

/// A single context record that informed the AI answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnswerContextRecord {
    /// What kind of context this record represents (e.g. "tenant_scope", "user_role", "permissions").
    pub record_type: String,
    /// Human-readable label or value for this context.
    pub label: String,
}

/// Trace entry for a tool invocation that informed the answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnswerToolTrace {
    /// Name of the tool that was invoked.
    pub tool_name: String,
}

/// Provider metadata captured from the turn that produced the answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnswerProviderTrace {
    /// Provider identifier (e.g. "fake", "glm_proxy").
    pub provider: String,
    /// Model identifier (e.g. "glm-4-flash").
    pub model: String,
    /// Route used to select the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_route: Option<String>,
    /// Why generation stopped (e.g. "stop").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Opaque provider request id for debugging.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
    /// End-to-end latency in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Token usage, if reported by the provider.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ProviderChatUsage>,
}

/// Answer provenance trace explaining which context records and tools
/// informed the AI assistant answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnswerTrace {
    /// Context records injected into the system prompt.
    pub context_records: Vec<AnswerContextRecord>,
    /// Tool invocations that informed the answer (empty until tool calling is wired).
    pub tools_used: Vec<AnswerToolTrace>,
    /// Provider metadata from the turn that produced the answer.
    pub provider: AnswerProviderTrace,
}

/// Response from Winston AI
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub conversation_id: Uuid,
    pub message: Message,
    pub trace: AnswerTrace,
}

/// Context injected into the AI agent
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub tenant_id: String,
    pub user_id: Uuid,
    pub user_role: String,
    pub permissions: Vec<String>,
    pub enabled_modules: Vec<billforge_core::Module>,
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

/// Route selector for provider model routing.
///
/// Each variant maps to a model resolved via [`AiModelConfig::model_for_route`]
/// or [`AiProvider::model_name_for_route`]. The [`Default`] variant is the
/// general-purpose chat model; the others allow callers to request a
/// faster, more capable, or tool-oriented model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderModelRoute {
    /// Fast / low-latency model for simple lookups.
    Fast,
    /// General-purpose chat model (the default).
    #[default]
    Default,
    /// Heavier reasoning model for complex tasks.
    Reasoning,
    /// Model optimised for tool/function calling turns.
    Tool,
}

/// A provider-neutral chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderChatRequest {
    /// Model identifier (e.g. "gpt-4o", "glm-4").
    pub model: String,
    /// Route that was used to select the model.
    #[serde(default)]
    pub model_route: ProviderModelRoute,
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
    /// Tool definitions to make available to the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ProviderToolDefinition>>,
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

/// A tool definition passed to a provider in a chat request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

/// A tool call emitted by the model in a response or stream chunk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// A single chunk in a provider streaming response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderChatStreamChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<ProviderChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<ProviderToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Bug report draft generation models
// ---------------------------------------------------------------------------

/// Request to generate a structured bug report draft from unstructured notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReportDraftRequest {
    /// Free-form bug description from the user.
    pub description: String,
    /// Optional conversation to attach context from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<Uuid>,
}

/// Priority level for a bug report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BugReportPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Structured bug report draft returned by Winston AI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BugReportDraftResponse {
    pub title: String,
    pub current_behavior: String,
    pub expected_behavior: String,
    pub reproduction_steps: Vec<String>,
    pub priority: BugReportPriority,
    pub affected_module: String,
    pub acceptance_criteria: Vec<String>,
}

// ---------------------------------------------------------------------------
// Feature request draft generation models
// ---------------------------------------------------------------------------

/// Request to generate a structured feature request draft from unstructured notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureRequestDraftRequest {
    /// Free-form feature request description from the user.
    pub description: String,
    /// Optional conversation to attach context from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<Uuid>,
}

/// Priority level for a feature request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FeatureRequestPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Structured feature request draft returned by Winston AI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureRequestDraftResponse {
    pub problem_statement: String,
    pub proposed_value: String,
    pub affected_module: String,
    pub priority: FeatureRequestPriority,
    pub acceptance_criteria: Vec<String>,
}
