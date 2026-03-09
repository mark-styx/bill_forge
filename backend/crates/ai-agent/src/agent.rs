//! Winston AI Agent - Main agent implementation

use anyhow::{Context, Result};
use async_openai::{
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
    Client,
};
use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use super::context::{build_system_prompt, inject_context};
use super::models::{AgentContext, ChatRequest, ChatResponse, Conversation, Message, MessageRole};
use super::tools::ToolRegistry;
use sqlx::PgPool;

/// Winston AI Agent
#[derive(Clone)]
pub struct WinstonAgent {
    pool: PgPool,
    tools: ToolRegistry,
}

impl WinstonAgent {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool: pool.clone(),
            tools: ToolRegistry::new(pool),
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

        // Build messages for OpenAI
        let system_prompt = build_system_prompt(&context);
        let mut messages = vec![
            ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(&system_prompt)
                    .build()?,
            ),
        ];

        // Add user message
        messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(request.message.clone())
                .build()?,
        ));

        // Create completion request
        let completion_request = CreateChatCompletionRequestArgs::default()
            .model("gpt-4-turbo-preview")
            .messages(messages)
            .temperature(0.7)
            .max_tokens(1000u16)
            .build()?;

        info!("Sending request to OpenAI for conversation {}", conversation_id);

        // Create client and call OpenAI API
        let client = Client::new();
        let response = client
            .chat()
            .create(completion_request)
            .await
            .context("OpenAI API call failed")?;

        // Extract assistant message
        let choice = response
            .choices
            .first()
            .context("No response from OpenAI")?;

        let assistant_content = choice
            .message
            .content
            .clone()
            .context("No content in OpenAI response")?;

        info!("Received response from OpenAI for conversation {}", conversation_id);

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
