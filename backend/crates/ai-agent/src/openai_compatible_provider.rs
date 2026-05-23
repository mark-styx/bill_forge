//! OpenAI-compatible provider adapter for the GLM proxy.
//!
//! Implements [`AiProvider`] by translating between provider-neutral types
//! and the `async-openai` SDK. Configuration is sourced from
//! [`AiProviderConfig`](crate::config::AiProviderConfig) which centralises
//! env-var parsing, validation, and defaults.

use async_openai::{
    types::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolType,
        CreateChatCompletionRequestArgs, FunctionObject,
    },
    Client,
};
use async_trait::async_trait;

use crate::config::AiProviderConfig;
use crate::models::*;
use crate::provider::{AiProvider, ProviderChatStream};

/// OpenAI-compatible provider that targets an OpenAI-style chat completions
/// endpoint (e.g. a GLM proxy).
///
/// Wraps an [`AiProviderConfig`] so all env-var parsing lives in one place.
pub struct OpenAiCompatibleProvider {
    provider_name: String,
    config: AiProviderConfig,
}

// Keep the old module-level env_keys around for the `from_env` compatibility
// wrapper and its test.
mod env_keys {
    pub const PROVIDER_NAME: &str = "WINSTON_AI_PROVIDER_NAME";
}

impl OpenAiCompatibleProvider {
    /// Build a provider from validated environment configuration.
    ///
    /// Uses [`AiProviderConfig::try_from_env`] under the hood, propagating
    /// validation errors for unsupported provider types or bad numeric values.
    pub fn try_from_env() -> Result<Self, crate::config::ConfigError> {
        let config = AiProviderConfig::try_from_env()?;
        let provider_name =
            std::env::var(env_keys::PROVIDER_NAME).unwrap_or_else(|_| "openai-compatible".into());
        Ok(Self {
            provider_name,
            config,
        })
    }

    /// Compatibility wrapper that panics on config errors.
    ///
    /// Prefer [`try_from_env()`](Self::try_from_env) in production code.
    pub fn from_env() -> Self {
        Self::try_from_env().expect("AI provider configuration is invalid")
    }

    /// Create with explicit config (useful for tests or programmatic setup).
    pub fn new(provider_name: String, model: String) -> Self {
        Self {
            provider_name,
            config: AiProviderConfig {
                provider_type: crate::config::AiProviderType::OpenAiCompatible,
                base_url: None,
                api_key: None,
                models: crate::config::AiModelConfig {
                    chat_model: model,
                    fast_model: None,
                    reasoning_model: None,
                    tool_model: None,
                    embedding_model: None,
                    max_tokens: 1000,
                },
                timeout: None,
            },
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
            config: AiProviderConfig {
                provider_type: crate::config::AiProviderType::OpenAiCompatible,
                base_url,
                api_key,
                models: crate::config::AiModelConfig {
                    chat_model: model,
                    fast_model: None,
                    reasoning_model: None,
                    tool_model: None,
                    embedding_model: None,
                    max_tokens: 1000,
                },
                timeout: None,
            },
        }
    }

    fn build_client(&self) -> Client<async_openai::config::OpenAIConfig> {
        let mut openai_config = async_openai::config::OpenAIConfig::new();
        if let Some(ref base_url) = self.config.base_url {
            openai_config = openai_config.with_api_base(base_url);
        }
        if let Some(ref api_key) = self.config.api_key {
            openai_config = openai_config.with_api_key(api_key);
        }
        let client = Client::with_config(openai_config);
        // Apply configured timeout to the underlying HTTP client when present.
        match self.config.timeout {
            Some(dur) => {
                let http = reqwest::Client::builder()
                    .timeout(dur)
                    .build()
                    .expect("reqwest client builder should not fail with valid timeout");
                client.with_http_client(http)
            }
            None => client,
        }
    }

    /// Convert provider-neutral messages into async-openai SDK message types.
    fn convert_messages(
        &self,
        messages: &[ProviderChatMessage],
    ) -> Result<Vec<ChatCompletionRequestMessage>, ProviderChatError> {
        let mut out = Vec::with_capacity(messages.len());
        for msg in messages {
            let converted = match msg.role {
                ProviderMessageRole::System => ChatCompletionRequestMessage::System(
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
                ),
                ProviderMessageRole::User => ChatCompletionRequestMessage::User(
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
                ),
                ProviderMessageRole::Assistant => ChatCompletionRequestMessage::Assistant(
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
                ),
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
        &self.config.models.chat_model
    }

    fn model_name_for_route(&self, route: ProviderModelRoute) -> &str {
        self.config.models.model_for_route(route)
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatResponse, ProviderChatError> {
        let sdk_messages = self.convert_messages(&request.messages)?;

        let mut req = CreateChatCompletionRequestArgs::default()
            .model(&request.model)
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
        // Use request max_tokens if provided, otherwise fall back to config default.
        // Config already validates max_tokens <= u16::MAX, so this cast is safe.
        if let Some(max_tokens) = request.max_tokens {
            req.max_tokens = Some(max_tokens.min(u16::MAX as u32) as u16);
        } else {
            req.max_tokens = Some(self.config.models.max_tokens as u16);
        }
        if let Some(tools) = request.tools {
            req.tools = Some(
                tools
                    .into_iter()
                    .map(|tool| ChatCompletionTool {
                        r#type: ChatCompletionToolType::Function,
                        function: FunctionObject {
                            name: tool.name,
                            description: tool.description,
                            parameters: Some(tool.parameters),
                        },
                    })
                    .collect(),
            );
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
        let tool_calls = choice.message.tool_calls.as_ref().map(|calls| {
            calls
                .iter()
                .map(|call| ProviderToolCall {
                    id: Some(call.id.clone()),
                    name: call.function.name.clone(),
                    arguments: serde_json::from_str(&call.function.arguments).unwrap_or_else(
                        |_| serde_json::Value::String(call.function.arguments.clone()),
                    ),
                })
                .collect()
        });

        Ok(ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content,
            },
            tool_calls,
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
            provider.config.base_url.as_deref(),
            Some("https://glm-proxy.example.com/v1")
        );
        assert_eq!(provider.config.api_key.as_deref(), Some("sk-test-key-123"));
    }

    /// `from_env` picks up Winston env vars when set, then falls back to
    /// defaults once they are removed. Combined into a single test to avoid
    /// parallel-test races on shared environment variables.
    #[test]
    fn from_env_reads_winston_vars_then_defaults() {
        let _lock = crate::config::ENV_LOCK.lock().unwrap();

        // Clear all AI-related env vars first to ensure a clean slate.
        for key in &[
            "WINSTON_AI_PROVIDER_TYPE",
            "WINSTON_AI_BASE_URL",
            "WINSTON_AI_API_KEY",
            "WINSTON_AI_DEFAULT_MODEL",
            "WINSTON_AI_CHAT_MODEL",
            "WINSTON_AI_MODEL",
            "WINSTON_AI_FAST_MODEL",
            "WINSTON_AI_REASONING_MODEL",
            "WINSTON_AI_TOOL_MODEL",
            "WINSTON_AI_EMBEDDING_MODEL",
            "WINSTON_AI_TIMEOUT_SECONDS",
            "WINSTON_AI_MAX_TOKENS",
            env_keys::PROVIDER_NAME,
        ] {
            std::env::remove_var(key);
        }

        // Phase 1: set vars, verify they are read.
        std::env::set_var(env_keys::PROVIDER_NAME, "glm-proxy");
        std::env::set_var("WINSTON_AI_CHAT_MODEL", "glm-4");
        std::env::set_var("WINSTON_AI_BASE_URL", "https://glm.local:8080/v1");
        std::env::set_var("WINSTON_AI_API_KEY", "sk-glm-key");

        let provider = OpenAiCompatibleProvider::from_env();
        assert_eq!(provider.provider_name(), "glm-proxy");
        assert_eq!(provider.model_name(), "glm-4");
        assert_eq!(
            provider.config.base_url.as_deref(),
            Some("https://glm.local:8080/v1")
        );
        assert_eq!(provider.config.api_key.as_deref(), Some("sk-glm-key"));

        // Phase 2: remove vars, verify defaults.
        std::env::remove_var(env_keys::PROVIDER_NAME);
        std::env::remove_var("WINSTON_AI_CHAT_MODEL");
        std::env::remove_var("WINSTON_AI_BASE_URL");
        std::env::remove_var("WINSTON_AI_API_KEY");

        let default_provider = OpenAiCompatibleProvider::from_env();
        assert_eq!(default_provider.provider_name(), "openai-compatible");
        assert_eq!(default_provider.model_name(), "gpt-4-turbo-preview");
        assert!(default_provider.config.base_url.is_none());
        assert!(default_provider.config.api_key.is_none());
    }

    /// `new()` (legacy constructor) leaves base_url and api_key as None.
    #[test]
    fn new_constructor_has_no_endpoint_config() {
        let provider = OpenAiCompatibleProvider::new("p".into(), "m".into());
        assert!(provider.config.base_url.is_none());
        assert!(provider.config.api_key.is_none());
    }
}
