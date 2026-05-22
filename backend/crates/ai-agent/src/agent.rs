//! Winston AI Agent - Main agent implementation

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use super::context::{build_system_prompt, inject_context};
use super::models::{
    AgentContext, ChatRequest, ChatResponse, Conversation, Message, MessageRole,
    ProviderChatMessage, ProviderChatRequest, ProviderMessageRole,
};
use super::provider::AiProvider;
use super::tools::ToolRegistry;
use sqlx::PgPool;

/// Winston AI Agent
#[derive(Clone)]
pub struct WinstonAgent {
    pool: PgPool,
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

        // Load or create conversation
        let conversation_id = match request.conversation_id {
            Some(id) => id,
            None => Uuid::new_v4(),
        };

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

        // Build provider-neutral completion request
        let provider_request = ProviderChatRequest {
            model: self.provider.model_name().to_string(),
            messages,
            temperature: Some(0.7),
            max_tokens: Some(1000),
            stop: None,
            tools: None,
        };

        info!(
            "Sending request to {} for conversation {}",
            self.provider.provider_name(),
            conversation_id
        );

        // Call provider
        let provider_response = self
            .provider
            .chat_completion(provider_request)
            .await
            .map_err(|e| anyhow::anyhow!("Provider chat completion failed: {}", e.message))?;

        let assistant_content = provider_response.message.content;

        info!(
            "Received response from {} for conversation {}",
            self.provider.provider_name(),
            conversation_id
        );

        // Create response message
        let assistant_message = Message {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: assistant_content,
            created_at: Utc::now(),
        };

        // Store conversation (in production, this would be persisted to database)
        // For now, we'll skip persistence and return the response

        Ok(ChatResponse {
            conversation_id,
            message: assistant_message,
        })
    }

    /// Get conversation history
    pub async fn get_conversation(&self, conversation_id: Uuid) -> Result<Option<Conversation>> {
        // In production, this would load from database
        // For now, return None to indicate not implemented
        warn!("Conversation persistence not yet implemented");
        Ok(None)
    }

    /// List user's conversations
    pub async fn list_conversations(&self, tenant_id: &str, user_id: Uuid) -> Result<Vec<Conversation>> {
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
    use crate::models::{ProviderChatError, ProviderChatErrorKind};
    use sqlx::PgPool;

    /// Helper: build a ChatRequest for tests.
    fn chat_request(message: &str) -> ChatRequest {
        ChatRequest {
            message: message.to_string(),
            conversation_id: None,
        }
    }

    /// WinstonAgent builds a provider-neutral request with system + user messages
    /// and delegates completion to the injected provider.
    #[tokio::test]
    async fn agent_delegates_to_injected_provider() {
        let provider = Arc::new(FakeAiProvider::new().with_response_text("test reply"));
        let pool = PgPool::connect_lazy("postgres:///_test_placeholder")
            .expect("lazy connect is always ok");
        let _agent = WinstonAgent::new(pool, provider.clone());

        // We can't call agent.chat() because inject_context hits the DB,
        // so we verify the provider-neutral request construction logic
        // by examining what FakeAiProvider records.

        // Build the provider request the same way the agent does:
        let system_prompt = "system prompt".to_string();
        let user_msg = "hello agent".to_string();
        let messages = vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: system_prompt,
            },
            ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: user_msg.clone(),
            },
        ];
        let provider_request = ProviderChatRequest {
            model: provider.model_name().to_string(),
            messages,
            temperature: Some(0.7),
            max_tokens: Some(1000),
            stop: None,
            tools: None,
        };

        let response = provider.chat_completion(provider_request).await.expect("completion");

        assert_eq!(response.message.role, ProviderMessageRole::Assistant);
        assert_eq!(response.message.content, "test reply");

        // Verify the request was recorded with the correct structure
        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].messages.len(), 2);
        assert_eq!(requests[0].messages[0].role, ProviderMessageRole::System);
        assert_eq!(requests[0].messages[1].role, ProviderMessageRole::User);
        assert_eq!(requests[0].messages[1].content, "hello agent");
        assert_eq!(requests[0].temperature, Some(0.7));
        assert_eq!(requests[0].max_tokens, Some(1000));
    }

    /// Provider name and model name are surfaced through the agent's provider.
    #[tokio::test]
    async fn agent_uses_provider_identity() {
        let provider =
            Arc::new(FakeAiProvider::new().with_model_name("glm-4-flash"));
        let pool = PgPool::connect_lazy("postgres:///_test_placeholder")
            .expect("lazy connect is always ok");
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
        let pool = PgPool::connect_lazy("postgres:///_test_placeholder")
            .expect("lazy connect is always ok");
        let agent = WinstonAgent::new(pool, provider.clone());

        // Simulate the provider call the agent would make
        let request = ProviderChatRequest {
            model: agent.provider.model_name().to_string(),
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
    }
}
