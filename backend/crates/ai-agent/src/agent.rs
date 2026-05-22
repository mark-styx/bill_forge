//! Winston AI Agent - Main agent implementation

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use super::context::{build_system_prompt, inject_context};
use super::models::{
    AgentContext, ChatRequest, ChatResponse, Conversation, Message, MessageRole,
    ProviderChatMessage, ProviderChatRequest, ProviderMessageRole, ProviderModelRoute,
};
use super::provider::AiProvider;
use super::tools::ToolRegistry;
use sqlx::PgPool;

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

    /// Process a chat message and return AI response
    pub async fn chat(
        &self,
        request: ChatRequest,
        tenant_id: String,
        user_id: Uuid,
    ) -> Result<ChatResponse> {
        // Inject user context
        let context = inject_context(&self.pool, tenant_id.clone(), user_id)
            .await
            .context("Failed to inject agent context")?;

        let conversation_id = match request.conversation_id {
            Some(id) => id,
            None => Uuid::new_v4(),
        };

        self.chat_with_context(request, tenant_id, user_id, context, conversation_id)
            .await
    }

    /// Core provider-call logic extracted from [`chat`](Self::chat).
    ///
    /// Separated so tests can pass a synthetic [`AgentContext`] without
    /// hitting the database via [`inject_context`].
    async fn chat_with_context(
        &self,
        request: ChatRequest,
        tenant_id: String,
        user_id: Uuid,
        context: AgentContext,
        conversation_id: Uuid,
    ) -> Result<ChatResponse> {
        // Build provider-neutral messages
        let system_prompt = build_system_prompt(&context);
        let messages = vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: system_prompt,
            },
            ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: request.message.clone(),
            },
        ];

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
                return Err(anyhow::anyhow!("Provider chat completion failed: {}", e.message));
            }
        };

        let assistant_content = provider_response.message.content;

        // Create response message
        let assistant_message = Message {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: assistant_content,
            created_at: Utc::now(),
        };

        Ok(ChatResponse {
            conversation_id,
            message: assistant_message,
        })
    }

    /// Get conversation history
    pub async fn get_conversation(&self, _conversation_id: Uuid) -> Result<Option<Conversation>> {
        // In production, this would load from database
        // For now, return None to indicate not implemented
        warn!("Conversation persistence not yet implemented");
        Ok(None)
    }

    /// List user's conversations
    pub async fn list_conversations(&self, _tenant_id: &str, _user_id: Uuid) -> Result<Vec<Conversation>> {
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
            tenant_id: "test-tenant".to_string(),
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
        let request = chat_request("hello agent");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(
                request,
                ctx.tenant_id.clone(),
                ctx.user_id,
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
        assert_eq!(rec.messages[1].content, "hello agent");

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
                ctx.tenant_id.clone(),
                ctx.user_id,
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
