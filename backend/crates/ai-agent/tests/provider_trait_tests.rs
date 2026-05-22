//! Tests for the AiProvider trait using the deterministic FakeAiProvider.

use futures::StreamExt;

use billforge_ai_agent::models::*;
use billforge_ai_agent::{AiProvider, FakeAiProvider};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn user_message(content: &str) -> ProviderChatMessage {
    ProviderChatMessage {
        role: ProviderMessageRole::User,
        content: content.into(),
    }
}

fn simple_request(content: &str) -> ProviderChatRequest {
    ProviderChatRequest {
        model: "fake-model".into(),
        model_route: ProviderModelRoute::Default,
        messages: vec![user_message(content)],
        temperature: None,
        max_tokens: None,
        stop: None,
        tools: None,
    }
}

// ---------------------------------------------------------------------------
// Trait identity tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trait_exposes_provider_and_model_names() {
    let p = FakeAiProvider::new();
    assert_eq!(p.provider_name(), "fake");
    assert_eq!(p.model_name(), "fake-model");
    assert!(!p.supports_tools());
}

#[tokio::test]
async fn trait_exposes_custom_model_name() {
    let p = FakeAiProvider::new().with_model_name("custom-v2");
    assert_eq!(p.model_name(), "custom-v2");
}

#[tokio::test]
async fn trait_exposes_supports_tools() {
    let p = FakeAiProvider::new().with_tools_supported(true);
    assert!(p.supports_tools());
}

// ---------------------------------------------------------------------------
// chat_completion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn chat_completion_echoes_last_message() {
    let p = FakeAiProvider::new();
    let response = p.chat_completion(simple_request("hello")).await.expect("completion");
    assert_eq!(response.message.role, ProviderMessageRole::Assistant);
    assert_eq!(response.message.content, "echo: hello");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    let usage = response.usage.expect("usage present");
    assert_eq!(usage.total_tokens, Some(15));
    assert_eq!(response.provider_request_id.as_deref(), Some("fake-req-001"));
}

#[tokio::test]
async fn chat_completion_with_custom_response_text() {
    let p = FakeAiProvider::new().with_response_text("fixed reply");
    let response = p.chat_completion(simple_request("anything")).await.expect("completion");
    assert_eq!(response.message.content, "fixed reply");
}

#[tokio::test]
async fn chat_completion_accepts_tools_and_returns_response() {
    let p = FakeAiProvider::new().with_tools_supported(true);

    let request = ProviderChatRequest {
        model: "fake-model".into(),
        model_route: ProviderModelRoute::Default,
        messages: vec![user_message("hello")],
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
async fn chat_completion_rejects_empty_messages() {
    let p = FakeAiProvider::new();
    let request = ProviderChatRequest {
        model: "fake-model".into(),
        model_route: ProviderModelRoute::Default,
        messages: vec![],
        temperature: None,
        max_tokens: None,
        stop: None,
        tools: None,
    };
    let err = p.chat_completion(request).await.expect_err("should fail");
    assert_eq!(err.kind, ProviderChatErrorKind::InvalidRequest);
    assert!(err.message.contains("empty"));
}

#[tokio::test]
async fn chat_completion_returns_configured_error() {
    let error = ProviderChatError {
        kind: ProviderChatErrorKind::RateLimit,
        message: "slow down".into(),
        status_code: Some(429),
        provider_code: None,
        retryable: Some(true),
    };
    let p = FakeAiProvider::new().with_error(error.clone());
    let err = p.chat_completion(simple_request("hi")).await.expect_err("should fail");
    assert_eq!(err, error);
}

// ---------------------------------------------------------------------------
// stream_chat_completion
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_chat_completion_returns_two_chunks() {
    let p = FakeAiProvider::new();
    let mut stream = p.stream_chat_completion(simple_request("stream me")).await.expect("stream");

    let chunk1 = stream.next().await.expect("first chunk").expect("ok");
    assert!(chunk1.delta.is_some());
    assert_eq!(chunk1.delta.as_ref().unwrap().content, "echo: stream me");
    assert!(chunk1.finish_reason.is_none());

    let chunk2 = stream.next().await.expect("second chunk").expect("ok");
    assert!(chunk2.delta.is_none());
    assert_eq!(chunk2.finish_reason.as_deref(), Some("stop"));

    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn stream_chat_completion_with_custom_response_text() {
    let p = FakeAiProvider::new().with_response_text("fixed stream");
    let mut stream = p.stream_chat_completion(simple_request("anything")).await.expect("stream");

    let chunk1 = stream.next().await.expect("first chunk").expect("ok");
    assert_eq!(chunk1.delta.unwrap().content, "fixed stream");
}

#[tokio::test]
async fn stream_chat_completion_rejects_empty_messages() {
    let p = FakeAiProvider::new();
    let request = ProviderChatRequest {
        model: "fake-model".into(),
        model_route: ProviderModelRoute::Default,
        messages: vec![],
        temperature: None,
        max_tokens: None,
        stop: None,
        tools: None,
    };
    let err = match p.stream_chat_completion(request).await {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert_eq!(err.kind, ProviderChatErrorKind::InvalidRequest);
}

#[tokio::test]
async fn stream_chat_completion_returns_configured_error() {
    let error = ProviderChatError {
        kind: ProviderChatErrorKind::Server,
        message: "internal".into(),
        status_code: Some(500),
        provider_code: None,
        retryable: Some(false),
    };
    let p = FakeAiProvider::new().with_error(error.clone());
    let err = match p.stream_chat_completion(simple_request("hi")).await {
        Err(e) => e,
        Ok(_) => panic!("expected error"),
    };
    assert_eq!(err, error);
}

// ---------------------------------------------------------------------------
// Request recording
// ---------------------------------------------------------------------------

#[tokio::test]
async fn records_chat_completion_requests() {
    let p = FakeAiProvider::new();
    p.chat_completion(simple_request("first")).await.unwrap();
    p.chat_completion(simple_request("second")).await.unwrap();

    let requests = p.take_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].messages[0].content, "first");
    assert_eq!(requests[1].messages[0].content, "second");
}

#[tokio::test]
async fn records_stream_requests() {
    let p = FakeAiProvider::new();
    let mut s = p.stream_chat_completion(simple_request("a")).await.unwrap();
    while s.next().await.is_some() {}
    let mut s2 = p.stream_chat_completion(simple_request("b")).await.unwrap();
    while s2.next().await.is_some() {}

    let requests = p.take_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].messages[0].content, "a");
    assert_eq!(requests[1].messages[0].content, "b");
}

#[tokio::test]
async fn take_requests_drains_log() {
    let p = FakeAiProvider::new();
    p.chat_completion(simple_request("x")).await.unwrap();
    assert_eq!(p.take_requests().len(), 1);
    assert_eq!(p.take_requests().len(), 0);
}

#[tokio::test]
async fn does_not_record_failed_validation() {
    let p = FakeAiProvider::new();
    let empty = ProviderChatRequest {
        model: "fake-model".into(),
        model_route: ProviderModelRoute::Default,
        messages: vec![],
        temperature: None,
        max_tokens: None,
        stop: None,
        tools: None,
    };
    let _ = p.chat_completion(empty).await;
    assert_eq!(p.take_requests().len(), 0);
}
