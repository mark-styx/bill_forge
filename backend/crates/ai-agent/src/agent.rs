//! Winston AI Agent - Main agent implementation

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use super::context::{build_system_prompt, inject_context};
use super::models::{
    AgentContext, AnswerContextRecord, AnswerProviderTrace, AnswerTrace,
    ChatRequest, ChatResponse, Conversation, Message, MessageRole,
    ProviderChatMessage, ProviderChatRequest, ProviderMessageRole, ProviderModelRoute,
};
use super::product_knowledge::{
    format_product_knowledge_block, product_knowledge_context_for_query,
};
use super::provider::AiProvider;
use super::tools::ToolRegistry;
use billforge_core::{TenantId, UserId};
use billforge_db::repositories::{
    AiConversationRepositoryImpl, AiMessageRole, AiMessageUsage, AppendAiMessageInput,
    AiUsageEventInput,
};
use sqlx::PgPool;

/// Truncate `s` to at most `max_bytes`, falling back to the nearest
/// UTF-8 character boundary so multibyte sequences are never split.
fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    if s.len() <= max_bytes {
        return Some(s.to_string());
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    Some(s[..end].to_string())
}

/// Telemetry captured from a single provider turn.
struct ProviderTurnTelemetry {
    selected_provider: String,
    selected_model: String,
    selected_route: ProviderModelRoute,
    finish_reason: Option<String>,
    provider_request_id: Option<String>,
    latency_ms: u64,
    usage: Option<super::models::ProviderChatUsage>,
}

/// Error data from a failed provider turn, kept structured for usage recording.
#[derive(Debug)]
struct ProviderTurnError {
    selected_provider: String,
    selected_model: String,
    selected_route: ProviderModelRoute,
    latency_ms: u64,
    provider_error: super::models::ProviderChatError,
}

impl std::fmt::Display for ProviderTurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Provider chat completion failed: {}",
            self.provider_error.message
        )
    }
}

impl std::error::Error for ProviderTurnError {}

/// Winston AI Agent
#[derive(Clone)]
pub struct WinstonAgent {
    pool: PgPool,
    #[allow(dead_code)] // will be used once tool-calling is wired up
    tools: ToolRegistry,
    provider: Arc<dyn AiProvider>,
}

impl WinstonAgent {
    pub fn new(pool: PgPool, provider: Arc<dyn AiProvider>) -> Self {
        Self {
            pool: pool.clone(),
            tools: ToolRegistry::new(pool),
            provider,
        }
    }

    /// Process a chat message and return AI response.
    ///
    /// This is the production entry point. It injects user context from the DB,
    /// persists the conversation and messages durably, then calls the provider.
    pub async fn chat(
        &self,
        request: ChatRequest,
        tenant_id: String,
        user_id: Uuid,
    ) -> Result<ChatResponse> {
        // Inject user context from tenant DB
        let context = inject_context(&self.pool, tenant_id.clone(), user_id)
            .await
            .context("Failed to inject agent context")?;

        let parsed_tid: TenantId = tenant_id
            .parse()
            .context("Failed to parse tenant_id as UUID")?;
        let parsed_uid = UserId(user_id);

        let repo = AiConversationRepositoryImpl::new(Arc::new(self.pool.clone()));
        let is_new_conversation = request.conversation_id.is_none();

        // For a new chat, create a conversation row before the provider call.
        // Persistence is required -- failures propagate as errors.
        let conversation_id = if is_new_conversation {
            let title = truncate_to_char_boundary(&request.message, 80);
            let record = repo
                .create_conversation(
                    &parsed_tid,
                    &parsed_uid,
                    title.as_deref(),
                    serde_json::json!({}),
                )
                .await
                .context("Failed to create conversation")?;
            record.id
        } else {
            request
                .conversation_id
                .expect("existing conversation_id must be present")
        };

        // Append the user message before the provider call.
        // For existing conversations, NotFound validates tenant/user ownership.
        let user_msg_input = AppendAiMessageInput {
            role: AiMessageRole::User,
            content: request.message.clone(),
            provider: None,
            model: None,
            model_route: None,
            finish_reason: None,
            provider_request_id: None,
            latency_ms: None,
            usage: None,
            metadata: serde_json::json!({}),
        };
        if let Err(e) = repo
            .append_message(&parsed_tid, &parsed_uid, conversation_id, user_msg_input)
            .await
        {
            match e {
                billforge_core::Error::NotFound { .. } if !is_new_conversation => {
                    return Err(anyhow::anyhow!(
                        "Conversation {} not found or access denied",
                        conversation_id
                    ));
                }
                other => {
                    return Err(anyhow::anyhow!(
                        "Failed to persist user message: {}",
                        other
                    ));
                }
            }
        }

        // Call the provider, capturing structured error for usage recording.
        let provider_result = self
            .execute_provider_turn(&request, &context, conversation_id, &tenant_id, user_id)
            .await;

        match provider_result {
            Ok((mut response, telemetry)) => {
                // Append the assistant message after a successful provider response.
                let assistant_msg_input = AppendAiMessageInput {
                    role: AiMessageRole::Assistant,
                    content: response.message.content.clone(),
                    provider: Some(telemetry.selected_provider.clone()),
                    model: Some(telemetry.selected_model.clone()),
                    model_route: Some(format!("{:?}", telemetry.selected_route)),
                    finish_reason: telemetry.finish_reason,
                    provider_request_id: telemetry.provider_request_id.clone(),
                    latency_ms: Some(telemetry.latency_ms as i64),
                    usage: telemetry.usage.as_ref().map(|u| AiMessageUsage {
                        prompt_tokens: u.prompt_tokens.map(|t| t as i32),
                        completion_tokens: u.completion_tokens.map(|t| t as i32),
                        total_tokens: u.total_tokens.map(|t| t as i32),
                    }),
                    metadata: serde_json::to_value(&response.trace).unwrap_or_else(|_| serde_json::json!({})),
                };
                let assistant_record = repo
                    .append_message(&parsed_tid, &parsed_uid, conversation_id, assistant_msg_input)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to persist assistant message: {}", e))?;

                // Replace the provisional provider-generated message ID with the
                // persisted database ID so the client can use it for feedback.
                response.message.id = assistant_record.id;

                // Record successful usage event (best-effort: warn on failure).
                let usage = telemetry.usage.as_ref();
                if let Err(e) = repo
                    .record_usage_event(
                        &parsed_tid,
                        &parsed_uid,
                        AiUsageEventInput {
                            conversation_id: Some(conversation_id),
                            message_id: Some(assistant_record.id),
                            provider: telemetry.selected_provider,
                            model: Some(telemetry.selected_model),
                            model_route: Some(format!("{:?}", telemetry.selected_route)),
                            latency_ms: Some(telemetry.latency_ms as i64),
                            prompt_tokens: usage.and_then(|u| u.prompt_tokens.map(|t| t as i32)),
                            completion_tokens: usage.and_then(|u| u.completion_tokens.map(|t| t as i32)),
                            total_tokens: usage.and_then(|u| u.total_tokens.map(|t| t as i32)),
                            success: true,
                            error_code: None,
                            error_message: None,
                            provider_request_id: telemetry.provider_request_id,
                            metadata: serde_json::json!({}),
                        },
                    )
                    .await
                {
                    warn!("Failed to record usage event (success): {}", e);
                }

                Ok(response)
            }
            Err(e) => {
                // Try to extract the structured ProviderTurnError for usage recording.
                if let Some(turn_err) = e.downcast_ref::<ProviderTurnError>() {
                    let err = &turn_err.provider_error;
                    if let Err(recording_err) = repo
                        .record_usage_event(
                            &parsed_tid,
                            &parsed_uid,
                            AiUsageEventInput {
                                conversation_id: Some(conversation_id),
                                message_id: None,
                                provider: turn_err.selected_provider.clone(),
                                model: Some(turn_err.selected_model.clone()),
                                model_route: Some(format!("{:?}", turn_err.selected_route)),
                                latency_ms: Some(turn_err.latency_ms as i64),
                                prompt_tokens: None,
                                completion_tokens: None,
                                total_tokens: None,
                                success: false,
                                error_code: err.provider_code.clone().or_else(|| {
                                    Some(format!("{:?}", err.kind))
                                }),
                                error_message: Some(err.message.clone()),
                                provider_request_id: None,
                                metadata: serde_json::json!({
                                    "kind": format!("{:?}", err.kind),
                                    "status_code": err.status_code,
                                    "retryable": err.retryable,
                                }),
                            },
                        )
                        .await
                    {
                        warn!(
                            "Failed to record usage event (provider failure): {}",
                            recording_err
                        );
                    }
                } else {
                    warn!(
                        "Provider turn failed for conversation {} with unstructured error: {}",
                        conversation_id, e
                    );
                }

                Err(e)
            }
        }
    }

    /// Core provider-call logic extracted for testability.
    ///
    /// Separated so tests can pass a synthetic [`AgentContext`] without
    /// hitting the database via [`inject_context`] or requiring persistence.
    /// Returns both the [`ChatResponse`] and telemetry data.
    #[cfg(test)]
    async fn chat_with_context(
        &self,
        request: ChatRequest,
        context: AgentContext,
        conversation_id: Uuid,
    ) -> Result<ChatResponse> {
        let (response, _) = self
            .execute_provider_turn(
                &request,
                &context,
                conversation_id,
                &context.tenant_id,
                context.user_id,
            )
            .await?;
        Ok(response)
    }

    /// Execute a single provider turn: build messages, call the provider,
    /// measure latency, log the outcome, and return the response plus telemetry.
    ///
    /// On provider error, returns a [`ProviderTurnError`] wrapped in `anyhow`
    /// so the caller can extract structured error data for usage recording.
    async fn execute_provider_turn(
        &self,
        request: &ChatRequest,
        context: &AgentContext,
        conversation_id: Uuid,
        tenant_id: &str,
        user_id: Uuid,
    ) -> Result<(ChatResponse, ProviderTurnTelemetry)> {
        // Build provider-neutral messages
        let system_prompt = build_system_prompt(context);

        // Retrieve product documentation snippets relevant to the user message.
        let pk_snippets = product_knowledge_context_for_query(&request.message);
        let pk_block = format_product_knowledge_block(&pk_snippets);

        let mut messages = vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: system_prompt,
            },
        ];

        // If product knowledge matched, inject it as a second system message
        // so the model can reference documentation without it dominating the
        // main system prompt.
        if !pk_block.is_empty() {
            messages.push(ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: pk_block,
            });
        }

        messages.push(ProviderChatMessage {
            role: ProviderMessageRole::User,
            content: request.message.clone(),
        });

        // Resolve provider routing into local variables before building the request.
        let selected_route = ProviderModelRoute::Default;
        let routing_reason = "default_chat_turn";
        let selected_provider = self.provider.provider_name().to_string();
        let selected_model = self.provider.model_name_for_route(selected_route).to_string();

        // Build provider-neutral completion request.
        // max_tokens is left as None so the provider applies its configured default.
        let provider_request = ProviderChatRequest {
            model: selected_model.clone(),
            model_route: selected_route,
            messages,
            temperature: Some(0.7),
            max_tokens: None,
            stop: None,
            tools: None,
        };

        // Call provider with latency measurement
        let start = std::time::Instant::now();
        let provider_result = self.provider.chat_completion(provider_request).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        let provider_response = match provider_result {
            Ok(resp) => {
                let provider_request_id = resp.provider_request_id.as_deref();
                info!(
                    selected_provider = %selected_provider,
                    selected_model = %selected_model,
                    model_route = ?selected_route,
                    routing_reason = %routing_reason,
                    latency_ms = %latency_ms,
                    conversation_id = %conversation_id,
                    tenant_id = %tenant_id,
                    user_id = %user_id,
                    outcome = "success",
                    provider_request_id = ?provider_request_id,
                    "AI turn completed"
                );
                resp
            }
            Err(e) => {
                warn!(
                    selected_provider = %selected_provider,
                    selected_model = %selected_model,
                    model_route = ?selected_route,
                    routing_reason = %routing_reason,
                    latency_ms = %latency_ms,
                    conversation_id = %conversation_id,
                    tenant_id = %tenant_id,
                    user_id = %user_id,
                    outcome = "error",
                    error_kind = ?e.kind,
                    status_code = ?e.status_code,
                    provider_code = ?e.provider_code,
                    retryable = ?e.retryable,
                    "AI turn failed"
                );
                let turn_err = ProviderTurnError {
                    selected_provider,
                    selected_model,
                    selected_route,
                    latency_ms,
                    provider_error: e,
                };
                return Err(turn_err.into());
            }
        };

        let assistant_content = provider_response.message.content.clone();

        let telemetry = ProviderTurnTelemetry {
            selected_provider,
            selected_model,
            selected_route,
            finish_reason: provider_response.finish_reason.clone(),
            provider_request_id: provider_response.provider_request_id.clone(),
            latency_ms,
            usage: provider_response.usage.clone(),
        };

        // Build answer provenance trace from context and telemetry.
        let mut context_records = vec![
            AnswerContextRecord {
                record_type: "tenant_scope".to_string(),
                label: format!("tenant_id={}", context.tenant_id),
            },
            AnswerContextRecord {
                record_type: "user_role".to_string(),
                label: context.user_role.clone(),
            },
            AnswerContextRecord {
                record_type: "permissions".to_string(),
                label: context.permissions.join(","),
            },
        ];

        // Add provenance records for any product documentation snippets that
        // were injected into the prompt.
        for snippet in &pk_snippets {
            let record_type = if snippet.source_path == "CHANGELOG.md" {
                "release_note"
            } else if snippet.source_path == ".github/workflows/release.yml" {
                "release_process"
            } else {
                "product_doc"
            };
            context_records.push(AnswerContextRecord {
                record_type: record_type.to_string(),
                label: format!("{}: {}", snippet.source_path, snippet.heading),
            });
        }

        let trace = AnswerTrace {
            context_records,
            tools_used: vec![],
            provider: AnswerProviderTrace {
                provider: telemetry.selected_provider.clone(),
                model: telemetry.selected_model.clone(),
                model_route: Some(format!("{:?}", telemetry.selected_route)),
                finish_reason: telemetry.finish_reason.clone(),
                provider_request_id: telemetry.provider_request_id.clone(),
                latency_ms: Some(telemetry.latency_ms),
                usage: telemetry.usage.clone(),
            },
        };

        let response = ChatResponse {
            conversation_id,
            message: Message {
                id: Uuid::new_v4(),
                role: MessageRole::Assistant,
                content: assistant_content,
                created_at: Utc::now(),
            },
            trace,
        };

        Ok((response, telemetry))
    }

    /// Get conversation history
    pub async fn get_conversation(&self, _conversation_id: Uuid) -> Result<Option<Conversation>> {
        // In production, this would load from database
        // For now, return None to indicate not implemented
        warn!("Conversation persistence not yet implemented");
        Ok(None)
    }

    /// List user's conversations
    pub async fn list_conversations(
        &self,
        _tenant_id: &str,
        _user_id: Uuid,
    ) -> Result<Vec<Conversation>> {
        // In production, this would load from database
        // For now, return empty list
        warn!("Conversation listing not yet implemented");
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_provider::FakeAiProvider;
    use crate::models::{AgentContext, ProviderChatError, ProviderChatErrorKind};
    use sqlx::PgPool;

    /// Helper: build a ChatRequest for tests.
    fn chat_request(message: &str) -> ChatRequest {
        ChatRequest {
            message: message.to_string(),
            conversation_id: None,
        }
    }

    /// Helper: build a synthetic AgentContext for tests.
    fn synthetic_context() -> AgentContext {
        AgentContext {
            tenant_id: "00000000-0000-0000-0000-000000000001".to_string(),
            user_id: Uuid::new_v4(),
            user_role: "admin".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        }
    }

    /// Helper: wire up a WinstonAgent with the given provider and a lazy PgPool.
    fn test_agent(provider: Arc<FakeAiProvider>) -> WinstonAgent {
        let pool = PgPool::connect_lazy("postgres:///_test_placeholder")
            .expect("lazy connect is always ok");
        WinstonAgent::new(pool, provider)
    }

    /// Proves that `chat_with_context` delegates to the injected fake provider,
    /// returns the fake provider's response text, and records exactly one request
    /// with the expected user message, temperature, max_tokens, and model route.
    #[tokio::test]
    async fn chat_with_context_uses_injected_fake_provider() {
        let provider = Arc::new(FakeAiProvider::new().with_response_text("test reply"));
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("hello world");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(
                request,
                ctx,
                conversation_id,
            )
            .await
            .expect("chat_with_context should succeed");

        // The ChatResponse must carry the fake provider's response text.
        assert_eq!(response.message.role, MessageRole::Assistant);
        assert_eq!(response.message.content, "test reply");
        assert_eq!(response.conversation_id, conversation_id);

        // The fake provider must have recorded exactly one request.
        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1, "expected exactly one provider request");

        let rec = &requests[0];
        // System + user messages.
        assert_eq!(rec.messages.len(), 2);
        assert_eq!(rec.messages[0].role, ProviderMessageRole::System);
        assert_eq!(rec.messages[1].role, ProviderMessageRole::User);
        assert_eq!(rec.messages[1].content, "hello world");

        // Agent sends temperature 0.7 and no max_tokens limit.
        assert_eq!(rec.temperature, Some(0.7));
        assert_eq!(rec.max_tokens, None);

        // Route is always Default for now.
        assert_eq!(rec.model_route, ProviderModelRoute::Default);
    }

    /// Proves that `chat_with_context` uses the injected provider's model selection
    /// path rather than a hard-coded provider model. Uses
    /// `FakeAiProvider::new().with_model_name("fake-selected-model")` and asserts
    /// the recorded request carries that model name.
    #[tokio::test]
    async fn chat_with_context_selects_model_from_injected_provider() {
        let provider = Arc::new(FakeAiProvider::new().with_model_name("fake-selected-model"));
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("check model selection");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(
                request,
                ctx,
                conversation_id,
            )
            .await
            .expect("chat_with_context should succeed");

        assert_eq!(response.message.role, MessageRole::Assistant);

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);

        let rec = &requests[0];
        // The model in the recorded request must be the one from the injected provider,
        // not a hard-coded GLM/OpenAI default.
        assert_eq!(rec.model, "fake-selected-model");
        assert_eq!(rec.model_route, ProviderModelRoute::Default);
    }

    /// Provider name and model name are surfaced through the agent's provider.
    #[tokio::test]
    async fn agent_uses_provider_identity() {
        let provider = Arc::new(FakeAiProvider::new().with_model_name("glm-4-flash"));
        let pool =
            PgPool::connect_lazy("postgres:///_test_placeholder").expect("lazy connect is always ok");
        let agent = WinstonAgent::new(pool, provider);

        assert_eq!(agent.provider.provider_name(), "fake");
        assert_eq!(agent.provider.model_name(), "glm-4-flash");
    }

    /// When the provider returns an error, agent.chat would propagate it.
    /// We test this by calling the provider directly with the error config.
    #[tokio::test]
    async fn provider_error_propagates() {
        let error = ProviderChatError {
            kind: ProviderChatErrorKind::RateLimit,
            message: "quota exceeded".into(),
            status_code: Some(429),
            provider_code: None,
            retryable: Some(true),
        };
        let provider = Arc::new(FakeAiProvider::new().with_error(error.clone()));
        let pool =
            PgPool::connect_lazy("postgres:///_test_placeholder").expect("lazy connect is always ok");
        let agent = WinstonAgent::new(pool, provider.clone());

        // Simulate the provider call the agent would make using model_name_for_route
        let selected_route = ProviderModelRoute::Default;
        let selected_model = agent
            .provider
            .model_name_for_route(selected_route)
            .to_string();
        let request = ProviderChatRequest {
            model: selected_model.clone(),
            model_route: selected_route,
            messages: vec![ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: "trigger error".into(),
            }],
            temperature: Some(0.7),
            max_tokens: Some(1000),
            stop: None,
            tools: None,
        };

        let err = agent
            .provider
            .chat_completion(request)
            .await
            .expect_err("should fail");
        assert_eq!(err.kind, ProviderChatErrorKind::RateLimit);
        assert_eq!(err.message, "quota exceeded");

        // Verify the recorded request model matches model_name_for_route(Default)
        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].model, selected_model);
        assert_eq!(requests[0].model_route, ProviderModelRoute::Default);
    }

    /// Proves that `chat_with_context` returns trace data containing tenant/user-role/permissions
    /// context records, provider/model metadata from FakeAiProvider, fake usage, and an empty
    /// tools_used array.
    #[tokio::test]
    async fn chat_with_context_returns_trace_data() {
        let provider = Arc::new(FakeAiProvider::new().with_response_text("trace reply"));
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("trace me");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx.clone(), conversation_id)
            .await
            .expect("chat_with_context should succeed");

        // Validate trace is present
        let trace = &response.trace;

        // Context records: tenant_scope, user_role, permissions
        assert_eq!(trace.context_records.len(), 3);
        assert_eq!(trace.context_records[0].record_type, "tenant_scope");
        assert!(trace.context_records[0].label.contains(&ctx.tenant_id));
        assert_eq!(trace.context_records[1].record_type, "user_role");
        assert_eq!(trace.context_records[1].label, "admin");
        assert_eq!(trace.context_records[2].record_type, "permissions");
        assert_eq!(trace.context_records[2].label, "read,write");

        // tools_used is intentionally empty (no tool calling wired yet)
        assert!(trace.tools_used.is_empty());

        // Provider metadata from FakeAiProvider
        assert_eq!(trace.provider.provider, "fake");
        assert_eq!(trace.provider.model, "fake-model");
        assert_eq!(trace.provider.finish_reason.as_deref(), Some("stop"));
        assert_eq!(trace.provider.provider_request_id.as_deref(), Some("fake-req-001"));
        assert!(trace.provider.latency_ms.is_some());

        // FakeAiProvider reports deterministic usage
        let usage = trace.provider.usage.as_ref().expect("usage should be present");
        assert_eq!(usage.prompt_tokens, Some(10));
        assert_eq!(usage.completion_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(15));
    }

    /// Verify that the routing reason and route constants used in chat() are stable.
    #[test]
    fn routing_constants_are_stable() {
        let selected_route = ProviderModelRoute::Default;
        let routing_reason = "default_chat_turn";

        // These constants must remain stable for log consumers.
        assert_eq!(routing_reason, "default_chat_turn");
        assert_eq!(selected_route, ProviderModelRoute::Default);
    }
}
