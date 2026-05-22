//! Integration tests for the GLM proxy provider adapter.
//!
//! Validates that environment-driven configuration produces the correct
//! provider type, model, base URL, API key, timeout, and max_tokens via the
//! public config and provider types, and that the streaming stub returns a
//! structured unsupported error.

use std::time::Duration;

use billforge_ai_agent::config::{AiProviderConfig, AiProviderType};
use billforge_ai_agent::models::*;
use billforge_ai_agent::{AiProvider, OpenAiCompatibleProvider};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Clear all Winston AI env vars to prevent cross-test leakage.
fn clear_env() {
    for key in [
        "WINSTON_AI_PROVIDER_TYPE",
        "WINSTON_AI_BASE_URL",
        "WINSTON_AI_API_KEY",
        "WINSTON_AI_CHAT_MODEL",
        "WINSTON_AI_MODEL",
        "WINSTON_AI_EMBEDDING_MODEL",
        "WINSTON_AI_TIMEOUT_SECONDS",
        "WINSTON_AI_MAX_TOKENS",
        "WINSTON_AI_PROVIDER_NAME",
    ] {
        std::env::remove_var(key);
    }
}

/// Serialized env-lock to prevent parallel races on environment mutation.
static ENV_LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(()));

fn user_message(content: &str) -> ProviderChatMessage {
    ProviderChatMessage {
        role: ProviderMessageRole::User,
        content: content.into(),
    }
}

fn simple_request(content: &str) -> ProviderChatRequest {
    ProviderChatRequest {
        model: "glm-4".into(),
        messages: vec![user_message(content)],
        temperature: None,
        max_tokens: None,
        stop: None,
        tools: None,
    }
}

// ---------------------------------------------------------------------------
// Config-level tests (using public AiProviderConfig)
// ---------------------------------------------------------------------------

#[test]
fn glm_proxy_env_produces_correct_config() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_env();

    std::env::set_var("WINSTON_AI_PROVIDER_TYPE", "glm-proxy");
    std::env::set_var("WINSTON_AI_CHAT_MODEL", "glm-4");
    std::env::set_var("WINSTON_AI_BASE_URL", "https://glm-proxy.example.com/v1");
    std::env::set_var("WINSTON_AI_API_KEY", "sk-glm-test-key");
    std::env::set_var("WINSTON_AI_TIMEOUT_SECONDS", "30");
    std::env::set_var("WINSTON_AI_MAX_TOKENS", "4096");

    let cfg = AiProviderConfig::try_from_env().expect("should parse");

    assert_eq!(cfg.provider_type, AiProviderType::OpenAiCompatible);
    assert_eq!(cfg.models.chat_model, "glm-4");
    assert_eq!(
        cfg.base_url.as_deref(),
        Some("https://glm-proxy.example.com/v1")
    );
    assert_eq!(cfg.api_key.as_deref(), Some("sk-glm-test-key"));
    assert_eq!(cfg.timeout, Some(Duration::from_secs(30)));
    assert_eq!(cfg.models.max_tokens, 4096);

    clear_env();
}

#[test]
fn glm_proxy_with_underscore_alias() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_env();

    std::env::set_var("WINSTON_AI_PROVIDER_TYPE", "glm_proxy");
    std::env::set_var("WINSTON_AI_CHAT_MODEL", "glm-4-flash");

    let cfg = AiProviderConfig::try_from_env().expect("should parse");
    assert_eq!(cfg.provider_type, AiProviderType::OpenAiCompatible);
    assert_eq!(cfg.models.chat_model, "glm-4-flash");

    clear_env();
}

#[test]
fn glm_proxy_defaults_when_minimal_env() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_env();

    // Only provider type; everything else should fall back to defaults.
    std::env::set_var("WINSTON_AI_PROVIDER_TYPE", "glm-proxy");

    let cfg = AiProviderConfig::try_from_env().expect("should parse");
    assert_eq!(cfg.provider_type, AiProviderType::OpenAiCompatible);
    assert_eq!(cfg.models.chat_model, "gpt-4-turbo-preview");
    assert!(cfg.base_url.is_none());
    assert!(cfg.api_key.is_none());
    assert!(cfg.timeout.is_none());
    assert_eq!(cfg.models.max_tokens, 1000);

    clear_env();
}

// ---------------------------------------------------------------------------
// Provider identity (uses public AiProvider trait methods only)
// ---------------------------------------------------------------------------

#[test]
fn glm_proxy_reports_model_and_no_tools() {
    let provider = OpenAiCompatibleProvider::with_config(
        "glm-proxy".into(),
        "glm-4".into(),
        Some("https://glm.local/v1".into()),
        Some("sk-key".into()),
    );

    assert_eq!(provider.provider_name(), "glm-proxy");
    assert_eq!(provider.model_name(), "glm-4");
    assert!(!provider.supports_tools());
}

#[test]
fn glm_proxy_from_env_reads_provider_name() {
    let _lock = ENV_LOCK.lock().unwrap();
    clear_env();

    std::env::set_var("WINSTON_AI_PROVIDER_NAME", "glm-proxy");
    std::env::set_var("WINSTON_AI_CHAT_MODEL", "glm-4");

    let provider = OpenAiCompatibleProvider::from_env();
    assert_eq!(provider.provider_name(), "glm-proxy");
    assert_eq!(provider.model_name(), "glm-4");

    clear_env();
}

// ---------------------------------------------------------------------------
// Streaming stub
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_chat_completion_returns_unsupported_error() {
    let provider = OpenAiCompatibleProvider::with_config(
        "glm-proxy".into(),
        "glm-4".into(),
        Some("https://glm.local/v1".into()),
        Some("sk-key".into()),
    );

    let result = provider
        .stream_chat_completion(simple_request("test"))
        .await;

    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected streaming to return an error"),
    };

    assert_eq!(err.kind, ProviderChatErrorKind::InvalidRequest);
    assert!(
        err.message.contains("Streaming is not yet supported"),
        "unexpected message: {:?}",
        err.message
    );
    assert_eq!(err.retryable, Some(false));
}
