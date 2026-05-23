//! Deterministic fake [`AiProvider`] implementation for unit and API tests.
//!
//! [`FakeAiProvider`] never hits the network. It records every request in a
//! thread-safe log, returns deterministic responses, and can be configured to
//! inject errors via a small builder API.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::models::*;
use crate::provider::{AiProvider, ProviderChatStream};

/// Deterministic, no-network [`AiProvider`] for testing.
///
/// # Defaults
///
/// | field            | value        |
/// |------------------|--------------|
/// | provider name    | `"fake"`     |
/// | model name       | `"fake-model"` |
/// | supports tools   | `false`      |
/// | response text    | `"echo: <last user message content>"` |
///
/// Use the builder methods to override any of these for a specific test case.
pub struct FakeAiProvider {
    model: String,
    tools_supported: bool,
    response_text: Option<String>,
    tool_calls: Option<Vec<ProviderToolCall>>,
    error: Option<ProviderChatError>,
    request_log: Arc<Mutex<Vec<ProviderChatRequest>>>,
}

impl Default for FakeAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeAiProvider {
    /// Create a fake provider with deterministic defaults.
    pub fn new() -> Self {
        Self {
            model: "fake-model".into(),
            tools_supported: false,
            response_text: None,
            tool_calls: None,
            error: None,
            request_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Override the model name returned by [`AiProvider::model_name`].
    pub fn with_model_name(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Enable or disable tool support.
    pub fn with_tools_supported(mut self, supported: bool) -> Self {
        self.tools_supported = supported;
        self
    }

    /// Use a fixed response text instead of the default echo behavior.
    pub fn with_response_text(mut self, text: impl Into<String>) -> Self {
        self.response_text = Some(text.into());
        self
    }

    /// Return deterministic tool calls when tools are offered in the request.
    pub fn with_tool_calls(mut self, tool_calls: Vec<ProviderToolCall>) -> Self {
        self.tool_calls = Some(tool_calls);
        self
    }

    /// Configure the provider to always return this error.
    pub fn with_error(mut self, error: ProviderChatError) -> Self {
        self.error = Some(error);
        self
    }

    /// Drain and return every request that was passed to
    /// [`chat_completion`](AiProvider::chat_completion) or
    /// [`stream_chat_completion`](AiProvider::stream_chat_completion).
    pub fn take_requests(&self) -> Vec<ProviderChatRequest> {
        self.request_log
            .lock()
            .expect("request_log lock poisoned")
            .drain(..)
            .collect()
    }

    fn log_request(&self, request: &ProviderChatRequest) {
        self.request_log
            .lock()
            .expect("request_log lock poisoned")
            .push(request.clone());
    }

    fn resolve_text(&self, request: &ProviderChatRequest) -> String {
        if let Some(ref text) = self.response_text {
            return text.clone();
        }
        let last = request
            .messages
            .last()
            .expect("messages non-empty (validated before)");
        format!("echo: {}", last.content)
    }

    fn check_messages(&self, request: &ProviderChatRequest) -> Result<(), ProviderChatError> {
        if request.messages.is_empty() {
            return Err(ProviderChatError {
                kind: ProviderChatErrorKind::InvalidRequest,
                message: "messages must not be empty".into(),
                status_code: None,
                provider_code: None,
                retryable: Some(false),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl AiProvider for FakeAiProvider {
    fn provider_name(&self) -> &str {
        "fake"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn supports_tools(&self) -> bool {
        self.tools_supported
    }

    async fn chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatResponse, ProviderChatError> {
        self.check_messages(&request)?;
        self.log_request(&request);

        if let Some(ref err) = self.error {
            return Err(err.clone());
        }

        let configured_tool_calls = if request.tools.is_some() {
            self.tool_calls.clone()
        } else {
            None
        };
        let finish_reason = if configured_tool_calls.is_some() {
            Some("tool_calls".into())
        } else {
            Some("stop".into())
        };
        let text = self.resolve_text(&request);
        Ok(ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: text,
            },
            tool_calls: configured_tool_calls,
            finish_reason,
            usage: Some(ProviderChatUsage {
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                total_tokens: Some(15),
            }),
            provider_request_id: Some("fake-req-001".into()),
        })
    }

    async fn stream_chat_completion(
        &self,
        request: ProviderChatRequest,
    ) -> Result<ProviderChatStream, ProviderChatError> {
        self.check_messages(&request)?;
        self.log_request(&request);

        if let Some(ref err) = self.error {
            return Err(err.clone());
        }

        let text = self.resolve_text(&request);

        let chunk1 = ProviderChatStreamChunk {
            delta: Some(ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: text,
            }),
            tool_call: None,
            finish_reason: None,
            provider_request_id: Some("fake-stream-001".into()),
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
