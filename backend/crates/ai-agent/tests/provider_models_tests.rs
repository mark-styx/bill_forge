//! Tests for the provider-neutral chat models (serialization round-trips).

use billforge_ai_agent::models::*;
use serde_json;

#[test]
fn provider_chat_request_serializes_with_system_and_user_messages() {
    let request = ProviderChatRequest {
        model: "test-model".into(),
        messages: vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: "You are a billing assistant.".into(),
            },
            ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: "What is invoice INV-001?".into(),
            },
        ],
        temperature: Some(0.7),
        max_tokens: Some(512),
        stop: None,
        tools: None,
    };

    let json = serde_json::to_string(&request).expect("serialize");
    let roundtrip: ProviderChatRequest = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(roundtrip, request);
    assert!(json.contains("\"model\""));
    assert!(json.contains("\"messages\""));
    assert!(json.contains("\"temperature\""));
    assert!(json.contains("\"max_tokens\""));
    // stop is None and should be omitted
    assert!(!json.contains("\"stop\""));
}

#[test]
fn provider_chat_request_deserializes_without_optional_fields() {
    let json = r#"{"model":"gpt-4o","messages":[{"role":"user","content":"hello"}]}"#;
    let req: ProviderChatRequest = serde_json::from_str(json).expect("deserialize");

    assert_eq!(req.model, "gpt-4o");
    assert_eq!(req.messages.len(), 1);
    assert_eq!(req.messages[0].role, ProviderMessageRole::User);
    assert!(req.temperature.is_none());
    assert!(req.max_tokens.is_none());
    assert!(req.stop.is_none());
    assert!(req.tools.is_none());
}

#[test]
fn provider_chat_response_round_trips_with_usage() {
    let response = ProviderChatResponse {
        message: ProviderChatMessage {
            role: ProviderMessageRole::Assistant,
            content: "Invoice INV-001 is approved.".into(),
        },
        finish_reason: Some("stop".into()),
        usage: Some(ProviderChatUsage {
            prompt_tokens: Some(20),
            completion_tokens: Some(10),
            total_tokens: Some(30),
        }),
        provider_request_id: Some("req-abc-123".into()),
    };

    let json = serde_json::to_string(&response).expect("serialize");
    let roundtrip: ProviderChatResponse = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(roundtrip, response);

    let usage = roundtrip.usage.unwrap();
    assert_eq!(usage.prompt_tokens, Some(20));
    assert_eq!(usage.completion_tokens, Some(10));
    assert_eq!(usage.total_tokens, Some(30));
}

#[test]
fn provider_chat_error_round_trips_with_all_fields() {
    let error = ProviderChatError {
        kind: ProviderChatErrorKind::RateLimit,
        message: "Too many requests".into(),
        status_code: Some(429),
        provider_code: Some("rate_limit_exceeded".into()),
        retryable: Some(true),
    };

    let json = serde_json::to_string(&error).expect("serialize");
    let roundtrip: ProviderChatError = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(roundtrip, error);
    assert_eq!(roundtrip.kind, ProviderChatErrorKind::RateLimit);
    assert_eq!(roundtrip.status_code, Some(429));
    assert_eq!(roundtrip.retryable, Some(true));
}

#[test]
fn provider_chat_error_minimal_fields() {
    let error = ProviderChatError {
        kind: ProviderChatErrorKind::Authentication,
        message: "Invalid API key".into(),
        status_code: None,
        provider_code: None,
        retryable: None,
    };

    let json = serde_json::to_string(&error).expect("serialize");
    let roundtrip: ProviderChatError = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(roundtrip.kind, ProviderChatErrorKind::Authentication);
    assert_eq!(roundtrip.message, "Invalid API key");
    assert!(roundtrip.status_code.is_none());
    assert!(roundtrip.provider_code.is_none());
    assert!(roundtrip.retryable.is_none());
}
