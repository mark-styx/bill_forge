//! Provider abstraction for AI chat completions.
//!
//! The [`AiProvider`] trait defines a narrow, provider-neutral interface that
//! concrete adapters (OpenAI, GLM proxy, etc.) implement. Downstream crates
//! depend on this trait rather than any specific SDK.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::models::{
    ProviderChatError, ProviderChatRequest, ProviderChatResponse, ProviderChatStreamChunk,
};

/// A boxed, provider-neutral stream of chat completion chunks.
pub type ProviderChatStream =
    Pin<Box<dyn Stream<Item = Result<ProviderChatStreamChunk, ProviderChatError>> + Send>>;

/// A provider-agnostic interface for LLM chat completions.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Human-readable provider identifier (e.g. `"openai"`, `"glm-proxy"`).
    fn provider_name(&self) -> &str;

    /// Model identifier forwarded in requests (e.g. `"gpt-4o"`, `"glm-4"`).
    fn model_name(&self) -> &str;

    /// Whether this provider supports tool/function calling.
    fn supports_tools(&self) -> bool;

    /// Perform a single-shot chat completion.
    async fn chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatResponse, ProviderChatError>;

    /// Perform a streaming chat completion, returning chunks as they arrive.
    async fn stream_chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatStream, ProviderChatError>;
}
