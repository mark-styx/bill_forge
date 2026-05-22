//! OpenAI-compatible provider adapter for the GLM proxy.
//!
//! Implements [`AiProvider`] by translating between provider-neutral types
//! and the `async-openai` SDK. Configuration is read from Winston-neutral
//! environment variables so no provider-specific knowledge leaks into the agent.

use std::env;

use async_openai::{
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
    },
    Client,
};
use async_trait::async_trait;

use crate::models::*;
use crate::provider::{AiProvider, ProviderChatStream};

/// Environment variable names consumed by the OpenAI-compatible adapter.
mod env_keys {
    pub const PROVIDER_NAME: &str = "WINSTON_AI_PROVIDER_NAME";
    pub const BASE_URL: &str = "WINSTON_AI_BASE_URL";
    pub const API_KEY: &str = "WINSTON_AI_API_KEY";
    pub const MODEL: &str = "WINSTON_AI_MODEL";
}

/// OpenAI-compatible provider that targets an OpenAI-style chat completions
/// endpoint (e.g. a GLM proxy). All config comes from environment variables.
pub struct OpenAiCompatibleProvider {
    provider_name: String,
    model: String,
    base_url: Option<String>,
    api_key: Option<String>,
}

impl OpenAiCompatibleProvider {
    /// Build a provider from environment variables.
    ///
    /// Falls back to sensible defaults when variables are unset:
    /// - `WINSTON_AI_PROVIDER_NAME` defaults to `"openai-compatible"`
    /// - `WINSTON_AI_MODEL` defaults to `"gpt-4-turbo-preview"`
    /// - `WINSTON_AI_BASE_URL` and `WINSTON_AI_API_KEY` are applied directly
    ///   to the `OpenAIConfig` when building the client.
    pub fn from_env() -> Self {
        let provider_name =
            env::var(env_keys::PROVIDER_NAME).unwrap_or_else(|_| "openai-compatible".into());
        let model = env::var(env_keys::MODEL).unwrap_or_else(|_| "gpt-4-turbo-preview".into());
        let base_url = env::var(env_keys::BASE_URL).ok();
        let api_key = env::var(env_keys::API_KEY).ok();

        Self {
            provider_name,
            model,
            base_url,
            api_key,
        }
    }

    /// Create with explicit config (useful for tests or programmatic setup).
    pub fn new(provider_name: String, model: String) -> Self {
        Self {
            provider_name,
            model,
            base_url: None,
            api_key: None,
        }
    }

    /// Create with explicit config including base URL and API key.
    pub fn with_config(
        provider_name: String,
        model: String,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            provider_name,
            model,
            base_url,
            api_key,
        }
    }

    fn build_client(&self) -> Client<async_openai::config::OpenAIConfig> {
        let mut config = async_openai::config::OpenAIConfig::new();
        if let Some(ref base_url) = self.base_url {
            config = config.with_api_base(base_url);
        }
        if let Some(ref api_key) = self.api_key {
            config = config.with_api_key(api_key);
        }
        Client::with_config(config)
    }

    /// Convert provider-neutral messages into async-openai SDK message types.
    fn convert_messages(
        &self,
        messages: &[ProviderChatMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>, ProviderChatError> {
        let mut out = Vec::with_capacity(messages.len());
        for msg in messages {
            let converted = match msg.role {
                ProviderMessageRole::System => {
                    ChatCompletionRequestMessage::System(
                        ChatCompletionRequestSystemMessageArgs::default()
                            .content(&*msg.content)
                            .build()
                            .map_err(|e| ProviderChatError {
                                kind: ProviderChatErrorKind::InvalidRequest,
                                message: format!("Failed to build system message: {}", e),
                                status_code: None,
                                provider_code: None,
                                retryable: Some(false),
                            })?,
                    )
                }
                ProviderMessageRole::User => {
                    ChatCompletionRequestMessage::User(
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(&*msg.content)
                            .build()
                            .map_err(|e| ProviderChatError {
                                kind: ProviderChatErrorKind::InvalidRequest,
                                message: format!("Failed to build user message: {}", e),
                                status_code: None,
                                provider_code: None,
                                retryable: Some(false),
                            })?,
                    )
                }
                ProviderMessageRole::Assistant => {
                    ChatCompletionRequestMessage::Assistant(
                        async_openai::types::ChatCompletionRequestAssistantMessageArgs::default()
                            .content(&*msg.content)
                            .build()
                            .map_err(|e| ProviderChatError {
                                kind: ProviderChatErrorKind::InvalidRequest,
                                message: format!("Failed to build assistant message: {}", e),
                                status_code: None,
                                provider_code: None,
                                retryable: Some(false),
                            })?,
                    )
                }
            };
            out.push(converted);
        }
        Ok(out)
    }
}

#[async_trait]
impl AiProvider for OpenAiCompatibleProvider {
    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn supports_tools(&self) -> bool {
        // Tool support can be enabled later; currently not wired through.
        false
    }

    async fn chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatResponse, ProviderChatError> {
        let sdk_messages = self.convert_messages(&request.messages)?;

        let mut req = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(sdk_messages)
            .build()
            .map_err(|e| ProviderChatError {
                kind: ProviderChatErrorKind::InvalidRequest,
                message: format!("Failed to build completion request: {}", e),
                status_code: None,
                provider_code: None,
                retryable: Some(false),
            })?;

        if let Some(temp) = request.temperature {
            req.temperature = Some(temp);
        }
        if let Some(max_tokens) = request.max_tokens {
            req.max_tokens = Some(max_tokens as u16);
        }

        let client = self.build_client();
        let response = client.chat().create(req).await.map_err(|e| {
            let message = format!("{}", e);
            let kind = if message.contains("rate limit") || message.contains("429") {
                ProviderChatErrorKind::RateLimit
            } else if message.contains("401") || message.contains("403") {
                ProviderChatErrorKind::Authentication
            } else if message.contains("404") || message.contains("model") {
                ProviderChatErrorKind::ModelNotFound
            } else if message.contains("context") || message.contains("token") {
                ProviderChatErrorKind::ContextLength
            } else {
                ProviderChatErrorKind::Server
            };
            let retryable = kind == ProviderChatErrorKind::RateLimit;
            ProviderChatError {
                kind,
                message,
                status_code: None,
                provider_code: None,
                retryable: Some(retryable),
            }
        })?;

        let choice = response.choices.first().ok_or_else(|| ProviderChatError {
            kind: ProviderChatErrorKind::Server,
            message: "No choices in provider response".into(),
            status_code: None,
            provider_code: None,
            retryable: Some(true),
        })?;

        let content = choice.message.content.clone().unwrap_or_default();
        let finish_reason = choice.finish_reason.as_ref().map(|r| format!("{:?}", r));

        Ok(ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content,
            },
            finish_reason,
            usage: response.usage.map(|u| ProviderChatUsage {
                prompt_tokens: Some(u.prompt_tokens as u32),
                completion_tokens: Some(u.completion_tokens as u32),
                total_tokens: Some(u.total_tokens as u32),
            }),
            provider_request_id: Some(response.id),
        })
    }

    async fn stream_chat_completion(
        &self,
        _request: ProviderChatRequest,
    ) -> Result<ProviderChatStream, ProviderChatError> {
        // Streaming is out of scope for this refactor. Return a structured error.
        Err(ProviderChatError {
            kind: ProviderChatErrorKind::InvalidRequest,
            message: "Streaming is not yet supported by the OpenAI-compatible adapter".into(),
            status_code: None,
            provider_code: None,
            retryable: Some(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `with_config` stores base_url and api_key so `build_client` uses them
    /// instead of the default OpenAI endpoint.
    #[test]
    fn with_config_stores_base_url_and_api_key() {
        let provider = OpenAiCompatibleProvider::with_config(
            "test-provider".into(),
            "glm-4-flash".into(),
            Some("https://glm-proxy.example.com/v1".into()),
            Some("sk-test-key-123".into()),
        );

        assert_eq!(provider.provider_name(), "test-provider");
        assert_eq!(provider.model_name(), "glm-4-flash");
        assert_eq!(
            provider.base_url.as_deref(),
            Some("https://glm-proxy.example.com/v1")
        );
        assert_eq!(provider.api_key.as_deref(), Some("sk-test-key-123"));
    }

    /// `from_env` picks up Winston env vars when set, then falls back to
    /// defaults once they are removed. Combined into a single test to avoid
    /// parallel-test races on shared environment variables.
    #[test]
    fn from_env_reads_winston_vars_then_defaults() {
        // Phase 1: set vars, verify they are read.
        env::set_var(env_keys::PROVIDER_NAME, "glm-proxy");
        env::set_var(env_keys::MODEL, "glm-4");
        env::set_var(env_keys::BASE_URL, "https://glm.local:8080/v1");
        env::set_var(env_keys::API_KEY, "sk-glm-key");

        let provider = OpenAiCompatibleProvider::from_env();
        assert_eq!(provider.provider_name(), "glm-proxy");
        assert_eq!(provider.model_name(), "glm-4");
        assert_eq!(provider.base_url.as_deref(), Some("https://glm.local:8080/v1"));
        assert_eq!(provider.api_key.as_deref(), Some("sk-glm-key"));

        // Phase 2: remove vars, verify defaults.
        env::remove_var(env_keys::PROVIDER_NAME);
        env::remove_var(env_keys::MODEL);
        env::remove_var(env_keys::BASE_URL);
        env::remove_var(env_keys::API_KEY);

        let default_provider = OpenAiCompatibleProvider::from_env();
        assert_eq!(default_provider.provider_name(), "openai-compatible");
        assert_eq!(default_provider.model_name(), "gpt-4-turbo-preview");
        assert!(default_provider.base_url.is_none());
        assert!(default_provider.api_key.is_none());
    }

    /// `new()` (legacy constructor) leaves base_url and api_key as None.
    #[test]
    fn new_constructor_has_no_endpoint_config() {
        let provider = OpenAiCompatibleProvider::new("p".into(), "m".into());
        assert!(provider.base_url.is_none());
        assert!(provider.api_key.is_none());
    }
}
