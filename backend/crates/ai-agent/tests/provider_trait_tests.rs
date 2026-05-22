//! Tests for the AiProvider trait using an in-memory mock implementation.

use async_trait::async_trait;
use futures::StreamExt;

use billforge_ai_agent::models::*;
use billforge_ai_agent::{AiProvider, ProviderChatStream};

// ---------------------------------------------------------------------------
// Mock provider
// ---------------------------------------------------------------------------

struct MockAiProvider {
    provider: &'static str,
    model: &'static str,
    tools_supported: bool,
}

#[async_trait]
impl AiProvider for MockAiProvider {
    fn provider_name(&self) -> &str {
        self.provider
    }

    fn model_name(&self) -> &str {
        self.model
    }

    fn supports_tools(&self) -> bool {
        self.tools_supported
    }

    async fn chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatResponse, ProviderChatError> {
        let last_msg = request.messages.last().expect("at least one message");
        Ok(ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: format!("echo: {}", last_msg.content),
            },
            finish_reason: Some("stop".into()),
            usage: Some(ProviderChatUsage {
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                total_tokens: Some(15),
            }),
            provider_request_id: Some("mock-req-001".into()),
        })
    }

    async fn stream_chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatStream, ProviderChatError> {
        let last_msg = request.messages.last().expect("at least one message");
        let content = format!("echo: {}", last_msg.content);

        // Emit two chunks: partial delta then final chunk with finish_reason.
        let chunk1 = ProviderChatStreamChunk {
            delta: Some(ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: content.clone(),
            }),
            tool_call: None,
            finish_reason: None,
            provider_request_id: Some("mock-stream-001".into()),
        };
        let chunk2 = ProviderChatStreamChunk {
            delta: None,
            tool_call: None,
            finish_reason: Some("stop".into()),
            provider_request_id: None,
        };

        let stream = futures::stream::iter(vec![Ok(chunk1), Ok(chunk2)]);
        Ok(Box::pin(stream))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trait_exposes_provider_and_model_names() {
    let p = MockAiProvider {
        provider: "mock",
        model: "mock-v1",
        tools_supported: false,
    };
    assert_eq!(p.provider_name(), "mock");
    assert_eq!(p.model_name(), "mock-v1");
    assert!(!p.supports_tools());
}

#[tokio::test]
async fn trait_exposes_supports_tools() {
    let p = MockAiProvider {
        provider: "mock",
        model: "mock-v1",
        tools_supported: true,
    };
    assert!(p.supports_tools());
}

#[tokio::test]
async fn chat_completion_accepts_tools_and_returns_response() {
    let p = MockAiProvider {
        provider: "mock",
        model: "mock-v1",
        tools_supported: true,
    };

    let request = ProviderChatRequest {
        model: "mock-v1".into(),
        messages: vec![ProviderChatMessage {
            role: ProviderMessageRole::User,
            content: "hello".into(),
        }],
        temperature: None,
        max_tokens: None,
        stop: None,
        tools: Some(vec![ProviderToolDefinition {
            name: "get_invoice".into(),
            description: Some("Fetch an invoice by ID".into()),
            parameters: serde_json::json!({"type": "object"}),
        }]),
    };

    let response = p.chat_completion(request).await.expect("completion");
    assert_eq!(response.message.role, ProviderMessageRole::Assistant);
    assert_eq!(response.message.content, "echo: hello");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    let usage = response.usage.expect("usage present");
    assert_eq!(usage.total_tokens, Some(15));
}

#[tokio::test]
async fn stream_chat_completion_returns_chunk_stream() {
    let p = MockAiProvider {
        provider: "mock",
        model: "mock-v1",
        tools_supported: false,
    };

    let request = ProviderChatRequest {
        model: "mock-v1".into(),
        messages: vec![ProviderChatMessage {
            role: ProviderMessageRole::User,
            content: "stream me".into(),
        }],
        temperature: None,
        max_tokens: None,
        stop: None,
        tools: None,
    };

    let mut stream = p.stream_chat_completion(request).await.expect("stream");

    let chunk1 = stream.next().await.expect("first chunk").expect("ok");
    assert!(chunk1.delta.is_some());
    assert_eq!(
        chunk1.delta.as_ref().unwrap().content,
        "echo: stream me"
    );
    assert!(chunk1.finish_reason.is_none());

    let chunk2 = stream.next().await.expect("second chunk").expect("ok");
    assert!(chunk2.delta.is_none());
    assert_eq!(chunk2.finish_reason.as_deref(), Some("stop"));

    // Stream should be exhausted.
    assert!(stream.next().await.is_none());
}
