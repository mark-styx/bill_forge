//! Winston AI Agent - Main agent implementation

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use super::context::{build_system_prompt, inject_context};
use super::models::{
    AgentContext, AnswerContextRecord, AnswerProviderTrace, AnswerToolTrace, AnswerTrace,
    BugReportDraftRequest, BugReportDraftResponse, ChatRequest, ChatResponse, Conversation,
    FeatureRequestDraftRequest, FeatureRequestDraftResponse, IssueSourceMetadata, Message,
    MessageRole, ProviderChatMessage, ProviderChatRequest, ProviderMessageRole, ProviderModelRoute,
};
use super::product_knowledge::{
    format_product_knowledge_block, product_knowledge_context_for_query,
};
use super::provider::AiProvider;
use super::tools::ToolRegistry;
use billforge_core::{TenantId, UserId};
use billforge_db::repositories::{
    AiConversationRepositoryImpl, AiMessageRole, AiMessageUsage, AiUsageEventInput,
    AppendAiMessageInput,
};
use sqlx::PgPool;

/// Maximum number of tool-execution iterations before forcing a final
/// text-only completion. This is the agentic-loop safety bound.
const MAX_TOOL_ITERATIONS: usize = 5;

/// Truncate `s` to at most `max_bytes`, falling back to the nearest
/// UTF-8 character boundary so multibyte sequences are never split.
fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    if s.len() <= max_bytes {
        return Some(s.to_string());
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    Some(s[..end].to_string())
}

/// Telemetry captured from a single provider turn.
struct ProviderTurnTelemetry {
    selected_provider: String,
    selected_model: String,
    selected_route: ProviderModelRoute,
    finish_reason: Option<String>,
    provider_request_id: Option<String>,
    latency_ms: u64,
    usage: Option<super::models::ProviderChatUsage>,
}

/// Error data from a failed provider turn, kept structured for usage recording.
#[derive(Debug)]
struct ProviderTurnError {
    selected_provider: String,
    selected_model: String,
    selected_route: ProviderModelRoute,
    latency_ms: u64,
    provider_error: super::models::ProviderChatError,
}

impl std::fmt::Display for ProviderTurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Provider chat completion failed: {}",
            self.provider_error.message
        )
    }
}

impl std::error::Error for ProviderTurnError {}

/// Winston AI Agent
#[derive(Clone)]
pub struct WinstonAgent {
    pool: PgPool,
    #[allow(dead_code)] // will be used once tool-calling is wired up
    tools: ToolRegistry,
    provider: Arc<dyn AiProvider>,
    enabled_modules: Vec<billforge_core::Module>,
}

impl WinstonAgent {
    pub fn new(pool: PgPool, provider: Arc<dyn AiProvider>) -> Self {
        Self {
            pool: pool.clone(),
            tools: ToolRegistry::new(pool),
            provider,
            enabled_modules: vec![],
        }
    }

    /// Set the tenant's enabled modules on this agent instance.
    pub fn with_enabled_modules(mut self, modules: Vec<billforge_core::Module>) -> Self {
        self.enabled_modules = modules;
        self
    }

    /// Process a chat message and return AI response.
    ///
    /// This is the production entry point. It injects user context from the DB,
    /// persists the conversation and messages durably, then calls the provider.
    pub async fn chat(
        &self,
        request: ChatRequest,
        tenant_id: String,
        user_id: Uuid,
    ) -> Result<ChatResponse> {
        // Inject user context from tenant DB
        let mut context = inject_context(&self.pool, tenant_id.clone(), user_id)
            .await
            .context("Failed to inject agent context")?;

        // Populate enabled modules from the authenticated tenant context.
        context.enabled_modules = self.enabled_modules.clone();

        let parsed_tid: TenantId = tenant_id
            .parse()
            .context("Failed to parse tenant_id as UUID")?;
        let parsed_uid = UserId(user_id);

        let repo = AiConversationRepositoryImpl::new(Arc::new(self.pool.clone()));
        let is_new_conversation = request.conversation_id.is_none();

        // For a new chat, create a conversation row before the provider call.
        // Persistence is required -- failures propagate as errors.
        let conversation_id = if is_new_conversation {
            let title = truncate_to_char_boundary(&request.message, 80);
            let record = repo
                .create_conversation(
                    &parsed_tid,
                    &parsed_uid,
                    title.as_deref(),
                    serde_json::json!({}),
                )
                .await
                .context("Failed to create conversation")?;
            record.id
        } else {
            request
                .conversation_id
                .expect("existing conversation_id must be present")
        };

        // Append the user message before the provider call.
        // For existing conversations, NotFound validates tenant/user ownership.
        let user_msg_input = AppendAiMessageInput {
            role: AiMessageRole::User,
            content: request.message.clone(),
            provider: None,
            model: None,
            model_route: None,
            finish_reason: None,
            provider_request_id: None,
            latency_ms: None,
            usage: None,
            metadata: serde_json::json!({}),
        };
        if let Err(e) = repo
            .append_message(&parsed_tid, &parsed_uid, conversation_id, user_msg_input)
            .await
        {
            match e {
                billforge_core::Error::NotFound { .. } if !is_new_conversation => {
                    return Err(anyhow::anyhow!(
                        "Conversation {} not found or access denied",
                        conversation_id
                    ));
                }
                other => {
                    return Err(anyhow::anyhow!("Failed to persist user message: {}", other));
                }
            }
        }

        // Call the provider, capturing structured error for usage recording.
        let provider_result = self
            .execute_provider_turn(&request, &context, conversation_id, &tenant_id, user_id)
            .await;

        match provider_result {
            Ok((mut response, telemetry)) => {
                // Append the assistant message after a successful provider response.
                let assistant_msg_input = AppendAiMessageInput {
                    role: AiMessageRole::Assistant,
                    content: response.message.content.clone(),
                    provider: Some(telemetry.selected_provider.clone()),
                    model: Some(telemetry.selected_model.clone()),
                    model_route: Some(format!("{:?}", telemetry.selected_route)),
                    finish_reason: telemetry.finish_reason,
                    provider_request_id: telemetry.provider_request_id.clone(),
                    latency_ms: Some(telemetry.latency_ms as i64),
                    usage: telemetry.usage.as_ref().map(|u| AiMessageUsage {
                        prompt_tokens: u.prompt_tokens.map(|t| t as i32),
                        completion_tokens: u.completion_tokens.map(|t| t as i32),
                        total_tokens: u.total_tokens.map(|t| t as i32),
                    }),
                    metadata: serde_json::to_value(&response.trace)
                        .unwrap_or_else(|_| serde_json::json!({})),
                };
                let assistant_record = repo
                    .append_message(
                        &parsed_tid,
                        &parsed_uid,
                        conversation_id,
                        assistant_msg_input,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to persist assistant message: {}", e))?;

                // Replace the provisional provider-generated message ID with the
                // persisted database ID so the client can use it for feedback.
                response.message.id = assistant_record.id;

                // Record successful usage event (best-effort: warn on failure).
                let usage = telemetry.usage.as_ref();
                if let Err(e) = repo
                    .record_usage_event(
                        &parsed_tid,
                        &parsed_uid,
                        AiUsageEventInput {
                            conversation_id: Some(conversation_id),
                            message_id: Some(assistant_record.id),
                            provider: telemetry.selected_provider,
                            model: Some(telemetry.selected_model),
                            model_route: Some(format!("{:?}", telemetry.selected_route)),
                            latency_ms: Some(telemetry.latency_ms as i64),
                            prompt_tokens: usage.and_then(|u| u.prompt_tokens.map(|t| t as i32)),
                            completion_tokens: usage
                                .and_then(|u| u.completion_tokens.map(|t| t as i32)),
                            total_tokens: usage.and_then(|u| u.total_tokens.map(|t| t as i32)),
                            success: true,
                            error_code: None,
                            error_message: None,
                            provider_request_id: telemetry.provider_request_id,
                            metadata: serde_json::json!({}),
                        },
                    )
                    .await
                {
                    warn!("Failed to record usage event (success): {}", e);
                }

                Ok(response)
            }
            Err(e) => {
                // Try to extract the structured ProviderTurnError for usage recording.
                if let Some(turn_err) = e.downcast_ref::<ProviderTurnError>() {
                    let err = &turn_err.provider_error;
                    if let Err(recording_err) = repo
                        .record_usage_event(
                            &parsed_tid,
                            &parsed_uid,
                            AiUsageEventInput {
                                conversation_id: Some(conversation_id),
                                message_id: None,
                                provider: turn_err.selected_provider.clone(),
                                model: Some(turn_err.selected_model.clone()),
                                model_route: Some(format!("{:?}", turn_err.selected_route)),
                                latency_ms: Some(turn_err.latency_ms as i64),
                                prompt_tokens: None,
                                completion_tokens: None,
                                total_tokens: None,
                                success: false,
                                error_code: err
                                    .provider_code
                                    .clone()
                                    .or_else(|| Some(format!("{:?}", err.kind))),
                                error_message: Some(err.message.clone()),
                                provider_request_id: None,
                                metadata: serde_json::json!({
                                    "kind": format!("{:?}", err.kind),
                                    "status_code": err.status_code,
                                    "retryable": err.retryable,
                                }),
                            },
                        )
                        .await
                    {
                        warn!(
                            "Failed to record usage event (provider failure): {}",
                            recording_err
                        );
                    }
                } else {
                    warn!(
                        "Provider turn failed for conversation {} with unstructured error: {}",
                        conversation_id, e
                    );
                }

                Err(e)
            }
        }
    }

    /// Core provider-call logic extracted for testability.
    ///
    /// Separated so tests can pass a synthetic [`AgentContext`] without
    /// hitting the database via [`inject_context`] or requiring persistence.
    /// Returns both the [`ChatResponse`] and telemetry data.
    ///
    /// This is intentionally `pub` so integration tests in `tests/` can
    /// exercise the agentic loop without a database.
    #[cfg(test)]
    pub async fn chat_with_context(
        &self,
        request: ChatRequest,
        context: AgentContext,
        conversation_id: Uuid,
    ) -> Result<ChatResponse> {
        let (response, _) = self
            .execute_provider_turn(
                &request,
                &context,
                conversation_id,
                &context.tenant_id,
                context.user_id,
            )
            .await?;
        Ok(response)
    }

    /// Execute a single provider turn: build messages, call the provider,
    /// measure latency, log the outcome, and return the response plus telemetry.
    ///
    /// On provider error, returns a [`ProviderTurnError`] wrapped in `anyhow`
    /// so the caller can extract structured error data for usage recording.
    async fn execute_provider_turn(
        &self,
        request: &ChatRequest,
        context: &AgentContext,
        conversation_id: Uuid,
        tenant_id: &str,
        user_id: Uuid,
    ) -> Result<(ChatResponse, ProviderTurnTelemetry)> {
        // Build provider-neutral messages
        let system_prompt = build_system_prompt(context);

        // Retrieve product documentation snippets relevant to the user message.
        let pk_snippets = product_knowledge_context_for_query(&request.message);
        let pk_block = format_product_knowledge_block(&pk_snippets);

        let mut messages = vec![ProviderChatMessage {
            role: ProviderMessageRole::System,
            content: system_prompt,
        }];

        // If product knowledge matched, inject it as a second system message
        // so the model can reference documentation without it dominating the
        // main system prompt.
        if !pk_block.is_empty() {
            messages.push(ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: pk_block,
            });
        }

        messages.push(ProviderChatMessage {
            role: ProviderMessageRole::User,
            content: request.message.clone(),
        });

        // Resolve provider routing into local variables before building the request.
        let selected_route = ProviderModelRoute::Default;
        let routing_reason = "default_chat_turn";
        let selected_provider = self.provider.provider_name().to_string();
        let selected_model = self
            .provider
            .model_name_for_route(selected_route)
            .to_string();
        let provider_tools = if self.provider.supports_tools() {
            Some(self.tools.provider_tool_definitions())
        } else {
            None
        };

        // Build provider-neutral completion request.
        // max_tokens is left as None so the provider applies its configured default.
        let provider_request = ProviderChatRequest {
            model: selected_model.clone(),
            model_route: selected_route,
            messages: messages.clone(),
            temperature: Some(0.7),
            max_tokens: None,
            stop: None,
            tools: provider_tools.clone(),
        };

        // Call provider with latency measurement
        let start = std::time::Instant::now();
        let provider_result = self.provider.chat_completion(provider_request).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        let provider_response = match provider_result {
            Ok(resp) => {
                let provider_request_id = resp.provider_request_id.as_deref();
                info!(
                    selected_provider = %selected_provider,
                    selected_model = %selected_model,
                    model_route = ?selected_route,
                    routing_reason = %routing_reason,
                    latency_ms = %latency_ms,
                    conversation_id = %conversation_id,
                    tenant_id = %tenant_id,
                    user_id = %user_id,
                    outcome = "success",
                    provider_request_id = ?provider_request_id,
                    "AI turn completed"
                );
                resp
            }
            Err(e) => {
                warn!(
                    selected_provider = %selected_provider,
                    selected_model = %selected_model,
                    model_route = ?selected_route,
                    routing_reason = %routing_reason,
                    latency_ms = %latency_ms,
                    conversation_id = %conversation_id,
                    tenant_id = %tenant_id,
                    user_id = %user_id,
                    outcome = "error",
                    error_kind = ?e.kind,
                    status_code = ?e.status_code,
                    provider_code = ?e.provider_code,
                    retryable = ?e.retryable,
                    "AI turn failed"
                );
                let turn_err = ProviderTurnError {
                    selected_provider,
                    selected_model,
                    selected_route,
                    latency_ms,
                    provider_error: e,
                };
                return Err(turn_err.into());
            }
        };

        let mut tool_traces = Vec::new();
        let mut iterations: usize = 0;
        let mut provider_response = provider_response;

        while provider_response.tool_calls.is_some() && iterations < MAX_TOOL_ITERATIONS {
            let tool_calls = provider_response.tool_calls.clone().unwrap();
            let mut tool_result_lines = Vec::new();
            for call in tool_calls {
                let args = match call.arguments {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                tool_traces.push(AnswerToolTrace {
                    tool_name: call.name.clone(),
                });
                let result = match self.tools.execute_tool(&call.name, context, &args).await {
                    Ok(result) => result,
                    Err(e) => format!("Tool error: {}", e),
                };
                tool_result_lines.push(format!("Tool {} returned:\n{}", call.name, result));
            }

            if !provider_response.message.content.is_empty() {
                messages.push(provider_response.message.clone());
            }
            messages.push(ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: format!(
                    "Use these read-only tool results to answer the user's original question. Do not claim any mutation occurred.\n\n{}",
                    tool_result_lines.join("\n\n")
                ),
            });

            let next_request = ProviderChatRequest {
                model: selected_model.clone(),
                model_route: selected_route,
                messages: messages.clone(),
                temperature: Some(0.7),
                max_tokens: None,
                stop: None,
                tools: provider_tools.clone(),
            };

            provider_response = self.provider.chat_completion(next_request).await.map_err(|e| {
                warn!(
                    selected_provider = %selected_provider,
                    selected_model = %selected_model,
                    model_route = ?selected_route,
                    conversation_id = %conversation_id,
                    tenant_id = %tenant_id,
                    user_id = %user_id,
                    error_kind = ?e.kind,
                    iteration = iterations,
                    "AI follow-up turn after tool call failed"
                );
                ProviderTurnError {
                    selected_provider: selected_provider.clone(),
                    selected_model: selected_model.clone(),
                    selected_route,
                    latency_ms,
                    provider_error: e,
                }
            })?;
            iterations += 1;
        }

        // If we hit the iteration cap and the model still wants tool calls,
        // force one final text-only completion so we never return tool calls
        // to the caller.
        if provider_response.tool_calls.is_some() {
            warn!(
                max_tool_iterations_reached = true,
                iterations = iterations,
                conversation_id = %conversation_id,
                "Agentic loop hit MAX_TOOL_ITERATIONS; forcing text-only terminator"
            );
            if !provider_response.message.content.is_empty() {
                messages.push(provider_response.message.clone());
            }
            let terminator_request = ProviderChatRequest {
                model: selected_model.clone(),
                model_route: selected_route,
                messages: messages.clone(),
                temperature: Some(0.7),
                max_tokens: None,
                stop: None,
                tools: None,
            };
            provider_response = self.provider.chat_completion(terminator_request).await.map_err(|e| {
                warn!(
                    selected_provider = %selected_provider,
                    selected_model = %selected_model,
                    model_route = ?selected_route,
                    conversation_id = %conversation_id,
                    tenant_id = %tenant_id,
                    user_id = %user_id,
                    error_kind = ?e.kind,
                    "AI terminator turn after max iterations failed"
                );
                ProviderTurnError {
                    selected_provider: selected_provider.clone(),
                    selected_model: selected_model.clone(),
                    selected_route,
                    latency_ms,
                    provider_error: e,
                }
            })?;
        }

        let assistant_content = provider_response.message.content.clone();

        let telemetry = ProviderTurnTelemetry {
            selected_provider,
            selected_model,
            selected_route,
            finish_reason: provider_response.finish_reason.clone(),
            provider_request_id: provider_response.provider_request_id.clone(),
            latency_ms,
            usage: provider_response.usage.clone(),
        };

        // Build answer provenance trace from context and telemetry.
        let mut context_records = vec![
            AnswerContextRecord {
                record_type: "tenant_scope".to_string(),
                label: format!("tenant_id={}", context.tenant_id),
            },
            AnswerContextRecord {
                record_type: "user_role".to_string(),
                label: context.user_role.clone(),
            },
            AnswerContextRecord {
                record_type: "permissions".to_string(),
                label: context.permissions.join(","),
            },
        ];

        // Add provenance records for any product documentation snippets that
        // were injected into the prompt.
        for snippet in &pk_snippets {
            let record_type = if snippet.source_path == "CHANGELOG.md" {
                "release_note"
            } else if snippet.source_path == ".github/workflows/release.yml" {
                "release_process"
            } else {
                "product_doc"
            };
            context_records.push(AnswerContextRecord {
                record_type: record_type.to_string(),
                label: format!("{}: {}", snippet.source_path, snippet.heading),
            });
        }

        let trace = AnswerTrace {
            context_records,
            tools_used: tool_traces,
            provider: AnswerProviderTrace {
                provider: telemetry.selected_provider.clone(),
                model: telemetry.selected_model.clone(),
                model_route: Some(format!("{:?}", telemetry.selected_route)),
                finish_reason: telemetry.finish_reason.clone(),
                provider_request_id: telemetry.provider_request_id.clone(),
                latency_ms: Some(telemetry.latency_ms),
                usage: telemetry.usage.clone(),
            },
        };

        let response = ChatResponse {
            conversation_id,
            message: Message {
                id: Uuid::new_v4(),
                role: MessageRole::Assistant,
                content: assistant_content,
                created_at: Utc::now(),
            },
            trace,
        };

        Ok((response, telemetry))
    }

    /// Generate a structured bug report draft from unstructured notes.
    ///
    /// Sends a focused JSON-only prompt to the provider and parses the
    /// response into a [`BugReportDraftResponse`]. Does not persist any
    /// records; this is purely a draft generation endpoint.
    pub async fn generate_bug_report_draft(
        &self,
        request: BugReportDraftRequest,
        tenant_id: String,
        user_id: Uuid,
    ) -> Result<BugReportDraftResponse> {
        let mut context = inject_context(&self.pool, tenant_id.clone(), user_id)
            .await
            .context("Failed to inject agent context")?;
        context.enabled_modules = self.enabled_modules.clone();

        self.generate_bug_report_draft_with_context(request, &context)
            .await
    }

    /// Inner method that builds prompt, calls provider, and parses JSON response.
    /// Extracted so tests can call it with a synthetic context, bypassing DB.
    async fn generate_bug_report_draft_with_context(
        &self,
        request: BugReportDraftRequest,
        context: &AgentContext,
    ) -> Result<BugReportDraftResponse> {
        let system_prompt = build_system_prompt(context);

        let json_schema_instruction = r#"You are a bug report drafting assistant. Given unstructured bug notes, produce a JSON object with EXACTLY these fields:
- "title": string - concise bug report title
- "current_behavior": string - what currently happens
- "expected_behavior": string - what should happen instead
- "reproduction_steps": array of strings - ordered steps to reproduce
- "priority": string - one of "low", "medium", "high", "critical"
- "affected_module": string - the system area or module affected
- "acceptance_criteria": array of strings - conditions that must be true for the bug to be considered fixed

Return ONLY the JSON object. No markdown fences, no explanation."#;

        let messages = vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: format!("{}\n\n{}", system_prompt, json_schema_instruction),
            },
            ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: request.description,
            },
        ];

        let selected_route = ProviderModelRoute::Default;
        let selected_model = self
            .provider
            .model_name_for_route(selected_route)
            .to_string();

        let provider_request = ProviderChatRequest {
            model: selected_model,
            model_route: selected_route,
            messages,
            temperature: Some(0.3),
            max_tokens: None,
            stop: None,
            tools: None,
        };

        let start = std::time::Instant::now();
        let provider_response = self.provider.chat_completion(provider_request).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        let provider_response = match provider_response {
            Ok(resp) => {
                info!(
                    selected_provider = %self.provider.provider_name(),
                    latency_ms = %latency_ms,
                    outcome = "bug_report_draft_success",
                    "Bug report draft generated"
                );
                resp
            }
            Err(e) => {
                warn!(
                    selected_provider = %self.provider.provider_name(),
                    latency_ms = %latency_ms,
                    outcome = "bug_report_draft_error",
                    error_kind = ?e.kind,
                    "Bug report draft generation failed"
                );
                return Err(anyhow::anyhow!("Provider error: {}", e.message));
            }
        };

        let content = provider_response.message.content.trim().to_string();

        // Strip markdown code fences if present
        let json_str = content
            .strip_prefix("```json")
            .or_else(|| content.strip_prefix("```"))
            .map(|s| s.strip_suffix("```").unwrap_or(s))
            .unwrap_or(&content)
            .trim();

        serde_json::from_str(json_str)
            .context("Failed to parse bug report draft from provider response. Expected valid JSON with title, current_behavior, expected_behavior, reproduction_steps, priority, affected_module, acceptance_criteria.")
            .map(|mut draft: BugReportDraftResponse| {
                draft.metadata = Self::build_source_metadata(request.conversation_id, "winston_ai", "bug");
                draft
            })
    }

    /// Generate a structured feature request draft from unstructured notes.
    ///
    /// Sends a focused JSON-only prompt to the provider and parses the
    /// response into a [`FeatureRequestDraftResponse`]. Does not persist any
    /// records; this is purely a draft generation endpoint.
    pub async fn generate_feature_request_draft(
        &self,
        request: FeatureRequestDraftRequest,
        tenant_id: String,
        user_id: Uuid,
    ) -> Result<FeatureRequestDraftResponse> {
        let mut context = inject_context(&self.pool, tenant_id.clone(), user_id)
            .await
            .context("Failed to inject agent context")?;
        context.enabled_modules = self.enabled_modules.clone();

        self.generate_feature_request_draft_with_context(request, &context)
            .await
    }

    /// Inner method that builds prompt, calls provider, and parses JSON response.
    /// Extracted so tests can call it with a synthetic context, bypassing DB.
    async fn generate_feature_request_draft_with_context(
        &self,
        request: FeatureRequestDraftRequest,
        context: &AgentContext,
    ) -> Result<FeatureRequestDraftResponse> {
        let system_prompt = build_system_prompt(context);

        let json_schema_instruction = r#"You are a feature request drafting assistant. Given unstructured feature request notes, produce a JSON object with EXACTLY these fields:
- "problem_statement": string - the problem or need the feature addresses
- "proposed_value": string - the proposed solution and its value
- "affected_module": string - the system area or module affected
- "priority": string - one of "low", "medium", "high", "critical"
- "acceptance_criteria": array of strings - conditions that must be true for the feature to be considered complete

Return ONLY the JSON object. No markdown fences, no explanation."#;

        let messages = vec![
            ProviderChatMessage {
                role: ProviderMessageRole::System,
                content: format!("{}\n\n{}", system_prompt, json_schema_instruction),
            },
            ProviderChatMessage {
                role: ProviderMessageRole::User,
                content: request.description,
            },
        ];

        let selected_route = ProviderModelRoute::Default;
        let selected_model = self
            .provider
            .model_name_for_route(selected_route)
            .to_string();

        let provider_request = ProviderChatRequest {
            model: selected_model,
            model_route: selected_route,
            messages,
            temperature: Some(0.3),
            max_tokens: None,
            stop: None,
            tools: None,
        };

        let start = std::time::Instant::now();
        let provider_response = self.provider.chat_completion(provider_request).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        let provider_response = match provider_response {
            Ok(resp) => {
                info!(
                    selected_provider = %self.provider.provider_name(),
                    latency_ms = %latency_ms,
                    outcome = "feature_request_draft_success",
                    "Feature request draft generated"
                );
                resp
            }
            Err(e) => {
                warn!(
                    selected_provider = %self.provider.provider_name(),
                    latency_ms = %latency_ms,
                    outcome = "feature_request_draft_error",
                    error_kind = ?e.kind,
                    "Feature request draft generation failed"
                );
                return Err(anyhow::anyhow!("Provider error: {}", e.message));
            }
        };

        let content = provider_response.message.content.trim().to_string();

        // Strip markdown code fences if present
        let json_str = content
            .strip_prefix("```json")
            .or_else(|| content.strip_prefix("```"))
            .map(|s| s.strip_suffix("```").unwrap_or(s))
            .unwrap_or(&content)
            .trim();

        serde_json::from_str(json_str)
            .context("Failed to parse feature request draft from provider response. Expected valid JSON with problem_statement, proposed_value, affected_module, priority, acceptance_criteria.")
            .map(|mut draft: FeatureRequestDraftResponse| {
                draft.metadata = Self::build_source_metadata(request.conversation_id, "winston_ai", "feature_request");
                draft
            })
    }

    /// Build source provenance metadata from an optional conversation_id.
    fn build_source_metadata(
        conversation_id: Option<Uuid>,
        intake_channel: &'static str,
        issue_kind: &'static str,
    ) -> Option<IssueSourceMetadata> {
        let source_conversation_link =
            conversation_id.map(|id| format!("/ai-assistant?conversation_id={}", id));
        Some(IssueSourceMetadata {
            source_conversation_id: conversation_id,
            source_conversation_link,
            intake_channel: intake_channel.to_string(),
            issue_kind: issue_kind.to_string(),
        })
    }

    /// Get conversation history
    pub async fn get_conversation(&self, _conversation_id: Uuid) -> Result<Option<Conversation>> {
        // In production, this would load from database
        // For now, return None to indicate not implemented
        warn!("Conversation persistence not yet implemented");
        Ok(None)
    }

    /// List user's conversations
    pub async fn list_conversations(
        &self,
        _tenant_id: &str,
        _user_id: Uuid,
    ) -> Result<Vec<Conversation>> {
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
    use crate::models::{
        AgentContext, BugReportDraftRequest, BugReportPriority, FeatureRequestDraftRequest,
        FeatureRequestPriority, ProviderChatError, ProviderChatErrorKind, ProviderChatResponse,
        ProviderToolCall,
    };
    use serde_json::json;
    use sqlx::PgPool;

    /// Helper: build a ChatRequest for tests.
    fn chat_request(message: &str) -> ChatRequest {
        ChatRequest {
            message: message.to_string(),
            conversation_id: None,
        }
    }

    /// Helper: build a synthetic AgentContext for tests.
    fn synthetic_context() -> AgentContext {
        AgentContext {
            tenant_id: "00000000-0000-0000-0000-000000000001".to_string(),
            user_id: Uuid::new_v4(),
            user_role: "admin".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
            enabled_modules: vec![],
        }
    }

    /// Helper: wire up a WinstonAgent with the given provider and a lazy PgPool.
    fn test_agent(provider: Arc<FakeAiProvider>) -> WinstonAgent {
        let pool = PgPool::connect_lazy("postgres:///_test_placeholder")
            .expect("lazy connect is always ok");
        WinstonAgent::new(pool, provider)
    }

    /// Proves that `chat_with_context` delegates to the injected fake provider,
    /// returns the fake provider's response text, and records exactly one request
    /// with the expected user message, temperature, max_tokens, and model route.
    #[tokio::test]
    async fn chat_with_context_uses_injected_fake_provider() {
        let provider = Arc::new(FakeAiProvider::new().with_response_text("test reply"));
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("hello world");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx, conversation_id)
            .await
            .expect("chat_with_context should succeed");

        // The ChatResponse must carry the fake provider's response text.
        assert_eq!(response.message.role, MessageRole::Assistant);
        assert_eq!(response.message.content, "test reply");
        assert_eq!(response.conversation_id, conversation_id);

        // The fake provider must have recorded exactly one request.
        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1, "expected exactly one provider request");

        let rec = &requests[0];
        // System + user messages.
        assert_eq!(rec.messages.len(), 2);
        assert_eq!(rec.messages[0].role, ProviderMessageRole::System);
        assert_eq!(rec.messages[1].role, ProviderMessageRole::User);
        assert_eq!(rec.messages[1].content, "hello world");

        // Agent sends temperature 0.7 and no max_tokens limit.
        assert_eq!(rec.temperature, Some(0.7));
        assert_eq!(rec.max_tokens, None);

        // Route is always Default for now.
        assert_eq!(rec.model_route, ProviderModelRoute::Default);
    }

    /// Proves that `chat_with_context` uses the injected provider's model selection
    /// path rather than a hard-coded provider model. Uses
    /// `FakeAiProvider::new().with_model_name("fake-selected-model")` and asserts
    /// the recorded request carries that model name.
    #[tokio::test]
    async fn chat_with_context_selects_model_from_injected_provider() {
        let provider = Arc::new(FakeAiProvider::new().with_model_name("fake-selected-model"));
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("check model selection");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx, conversation_id)
            .await
            .expect("chat_with_context should succeed");

        assert_eq!(response.message.role, MessageRole::Assistant);

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);

        let rec = &requests[0];
        // The model in the recorded request must be the one from the injected provider,
        // not a hard-coded GLM/OpenAI default.
        assert_eq!(rec.model, "fake-selected-model");
        assert_eq!(rec.model_route, ProviderModelRoute::Default);
    }

    /// Provider name and model name are surfaced through the agent's provider.
    #[tokio::test]
    async fn agent_uses_provider_identity() {
        let provider = Arc::new(FakeAiProvider::new().with_model_name("glm-4-flash"));
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

        // Simulate the provider call the agent would make using model_name_for_route
        let selected_route = ProviderModelRoute::Default;
        let selected_model = agent
            .provider
            .model_name_for_route(selected_route)
            .to_string();
        let request = ProviderChatRequest {
            model: selected_model.clone(),
            model_route: selected_route,
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

        // Verify the recorded request model matches model_name_for_route(Default)
        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].model, selected_model);
        assert_eq!(requests[0].model_route, ProviderModelRoute::Default);
    }

    /// Proves that `chat_with_context` returns trace data containing tenant/user-role/permissions
    /// context records, provider/model metadata from FakeAiProvider, fake usage, and an empty
    /// tools_used array.
    #[tokio::test]
    async fn chat_with_context_returns_trace_data() {
        let provider = Arc::new(FakeAiProvider::new().with_response_text("trace reply"));
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("trace me");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx.clone(), conversation_id)
            .await
            .expect("chat_with_context should succeed");

        // Validate trace is present
        let trace = &response.trace;

        // Context records: tenant_scope, user_role, permissions
        assert_eq!(trace.context_records.len(), 3);
        assert_eq!(trace.context_records[0].record_type, "tenant_scope");
        assert!(trace.context_records[0].label.contains(&ctx.tenant_id));
        assert_eq!(trace.context_records[1].record_type, "user_role");
        assert_eq!(trace.context_records[1].label, "admin");
        assert_eq!(trace.context_records[2].record_type, "permissions");
        assert_eq!(trace.context_records[2].label, "read,write");

        // tools_used is intentionally empty (no tool calling wired yet)
        assert!(trace.tools_used.is_empty());

        // Provider metadata from FakeAiProvider
        assert_eq!(trace.provider.provider, "fake");
        assert_eq!(trace.provider.model, "fake-model");
        assert_eq!(trace.provider.finish_reason.as_deref(), Some("stop"));
        assert_eq!(
            trace.provider.provider_request_id.as_deref(),
            Some("fake-req-001")
        );
        assert!(trace.provider.latency_ms.is_some());

        // FakeAiProvider reports deterministic usage
        let usage = trace
            .provider
            .usage
            .as_ref()
            .expect("usage should be present");
        assert_eq!(usage.prompt_tokens, Some(10));
        assert_eq!(usage.completion_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(15));
    }

    #[tokio::test]
    async fn chat_with_context_executes_read_only_tool_directly() {
        let tool_call_response = ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: String::new(),
            },
            tool_calls: Some(vec![ProviderToolCall {
                id: Some("call-001".to_string()),
                name: "get_module_capabilities".to_string(),
                arguments: json!({}),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: None,
            provider_request_id: Some("fake-req-001".to_string()),
        };
        let final_response = ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: "final answer".to_string(),
            },
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            provider_request_id: Some("fake-req-002".to_string()),
        };
        let provider = Arc::new(
            FakeAiProvider::new()
                .with_tools_supported(true)
                .with_response_sequence(vec![tool_call_response, final_response]),
        );
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("What can Winston do?");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx, conversation_id)
            .await
            .expect("chat_with_context should succeed");

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 2, "expected tool turn and follow-up turn");
        assert!(requests[0]
            .tools
            .as_ref()
            .expect("initial request should include tools")
            .iter()
            .any(|tool| tool.name == "get_module_capabilities"));
        // After the agentic-loop change, the follow-up request carries tools
        // so the model can request additional tools on the next hop.
        assert!(
            requests[1].tools.is_some(),
            "follow-up request should include tools for agentic iteration"
        );

        let tool_result_message = requests[1]
            .messages
            .last()
            .expect("follow-up request should include tool result message");
        assert_eq!(tool_result_message.role, ProviderMessageRole::System);
        assert!(tool_result_message
            .content
            .contains("Tool get_module_capabilities returned:"));
        assert!(tool_result_message
            .content
            .contains("Module Capabilities Report"));
        assert!(tool_result_message
            .content
            .contains("Winston AI Assistant is a paid add-on"));

        assert_eq!(response.trace.tools_used.len(), 1);
        assert_eq!(
            response.trace.tools_used[0].tool_name,
            "get_module_capabilities"
        );
    }

    #[tokio::test]
    async fn chat_with_context_blocks_mutating_tool_from_normal_turn() {
        let tool_call_response = ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: String::new(),
            },
            tool_calls: Some(vec![ProviderToolCall {
                id: Some("call-001".to_string()),
                name: "synthetic_mutating_test_tool".to_string(),
                arguments: json!({}),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: None,
            provider_request_id: Some("fake-req-001".to_string()),
        };
        let final_response = ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: "final answer".to_string(),
            },
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            provider_request_id: Some("fake-req-002".to_string()),
        };
        let provider = Arc::new(
            FakeAiProvider::new()
                .with_tools_supported(true)
                .with_response_sequence(vec![tool_call_response, final_response]),
        );
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("Please make a change.");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx, conversation_id)
            .await
            .expect("chat_with_context should succeed");

        let requests = provider.take_requests();
        assert_eq!(requests.len(), 2, "expected tool turn and follow-up turn");
        assert!(requests[0]
            .tools
            .as_ref()
            .expect("initial request should include tools")
            .iter()
            .any(|tool| tool.name == "synthetic_mutating_test_tool"));
        // After the agentic-loop change, the follow-up request carries tools
        // so the model can request additional tools on the next hop.
        assert!(
            requests[1].tools.is_some(),
            "follow-up request should include tools for agentic iteration"
        );

        let tool_result_message = requests[1]
            .messages
            .last()
            .expect("follow-up request should include guard result message");
        assert_eq!(tool_result_message.role, ProviderMessageRole::System);
        assert!(tool_result_message.content.contains(
            "Tool 'synthetic_mutating_test_tool' requires an approved proposal context before execution"
        ));
        assert!(tool_result_message
            .content
            .contains("Do not claim any mutation occurred."));
        assert!(
            !tool_result_message.content.contains("not found"),
            "guard should reject before dispatch when no approved proposal context is supplied"
        );

        assert_eq!(response.trace.tools_used.len(), 1);
        assert_eq!(
            response.trace.tools_used[0].tool_name,
            "synthetic_mutating_test_tool"
        );
    }

    /// Verify that the routing reason and route constants used in chat() are stable.
    #[test]
    fn routing_constants_are_stable() {
        let selected_route = ProviderModelRoute::Default;
        let routing_reason = "default_chat_turn";

        // These constants must remain stable for log consumers.
        assert_eq!(routing_reason, "default_chat_turn");
        assert_eq!(selected_route, ProviderModelRoute::Default);
    }

    // -------------------------------------------------------------------------
    // Bug report draft generation tests
    // -------------------------------------------------------------------------

    /// Helper: build a BugReportDraftRequest for tests.
    fn bug_report_request(description: &str) -> BugReportDraftRequest {
        BugReportDraftRequest {
            description: description.to_string(),
            conversation_id: None,
        }
    }

    /// Proves that generate_bug_report_draft sends a prompt requesting all required
    /// fields and successfully parses valid provider JSON into BugReportDraftResponse.
    #[tokio::test]
    async fn bug_report_draft_parses_valid_json() {
        let valid_json = serde_json::json!({
            "title": "Login page crashes on submit",
            "current_behavior": "Page shows a white screen after clicking login",
            "expected_behavior": "User is redirected to the dashboard",
            "reproduction_steps": ["Go to /login", "Enter credentials", "Click submit"],
            "priority": "high",
            "affected_module": "Authentication",
            "acceptance_criteria": [
                "Login submits without crash",
                "User sees dashboard after login",
                "Error messages display correctly"
            ]
        })
        .to_string();

        let provider = Arc::new(FakeAiProvider::new().with_response_text(valid_json));
        let agent = test_agent(provider.clone());

        let request = bug_report_request("Login page crashes when I click submit. White screen.");
        let ctx = synthetic_context();

        let draft = agent
            .generate_bug_report_draft_with_context(request, &ctx)
            .await
            .expect("draft generation should succeed");

        // Verify all fields parsed correctly
        assert_eq!(draft.title, "Login page crashes on submit");
        assert_eq!(
            draft.current_behavior,
            "Page shows a white screen after clicking login"
        );
        assert_eq!(
            draft.expected_behavior,
            "User is redirected to the dashboard"
        );
        assert_eq!(draft.reproduction_steps.len(), 3);
        assert_eq!(draft.reproduction_steps[0], "Go to /login");
        assert_eq!(draft.priority, BugReportPriority::High);
        assert_eq!(draft.affected_module, "Authentication");
        assert_eq!(draft.acceptance_criteria.len(), 3);

        // Verify the provider received the prompt with bug-report-specific instructions
        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);
        let system_msg = &requests[0].messages[0];
        assert!(system_msg.content.contains("title"));
        assert!(system_msg.content.contains("current_behavior"));
        assert!(system_msg.content.contains("expected_behavior"));
        assert!(system_msg.content.contains("reproduction_steps"));
        assert!(system_msg.content.contains("priority"));
        assert!(system_msg.content.contains("affected_module"));
        assert!(system_msg.content.contains("acceptance_criteria"));
        // Lower temperature for structured output
        assert_eq!(requests[0].temperature, Some(0.3));
    }

    /// Proves that generate_bug_report_draft returns an error when the provider
    /// returns malformed JSON (e.g. freeform text instead of JSON).
    #[tokio::test]
    async fn bug_report_draft_rejects_malformed_json() {
        let provider = Arc::new(
            FakeAiProvider::new()
                .with_response_text("This is not JSON, just freeform text about a bug."),
        );
        let agent = test_agent(provider);

        let request = bug_report_request("Something is broken.");
        let ctx = synthetic_context();

        let result = agent
            .generate_bug_report_draft_with_context(request, &ctx)
            .await;

        assert!(result.is_err(), "should fail for malformed JSON");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Failed to parse bug report draft"),
            "error message should mention parse failure, got: {}",
            err
        );
    }

    /// Proves that generate_bug_report_draft handles JSON wrapped in markdown
    /// code fences (common provider behavior).
    #[tokio::test]
    async fn bug_report_draft_strips_markdown_fences() {
        let raw_json = serde_json::json!({
            "title": "Slow dashboard load",
            "current_behavior": "Dashboard takes 30 seconds to load",
            "expected_behavior": "Dashboard loads in under 3 seconds",
            "reproduction_steps": ["Open the app", "Navigate to dashboard"],
            "priority": "medium",
            "affected_module": "Reporting",
            "acceptance_criteria": ["Dashboard loads in < 3s"]
        })
        .to_string();

        // Wrap in markdown fences like many LLMs do
        let fenced = format!("```json\n{}\n```", raw_json);

        let provider = Arc::new(FakeAiProvider::new().with_response_text(fenced));
        let agent = test_agent(provider);

        let request = bug_report_request("Dashboard is really slow");
        let ctx = synthetic_context();

        let draft = agent
            .generate_bug_report_draft_with_context(request, &ctx)
            .await
            .expect("should strip fences and parse");

        assert_eq!(draft.title, "Slow dashboard load");
        assert_eq!(draft.priority, BugReportPriority::Medium);
    }

    // -------------------------------------------------------------------------
    // Feature request draft generation tests
    // -------------------------------------------------------------------------

    /// Helper: build a FeatureRequestDraftRequest for tests.
    fn feature_request_request(description: &str) -> FeatureRequestDraftRequest {
        FeatureRequestDraftRequest {
            description: description.to_string(),
            conversation_id: None,
        }
    }

    /// Proves that generate_feature_request_draft sends a prompt requesting all
    /// required fields and successfully parses valid provider JSON into
    /// FeatureRequestDraftResponse.
    #[tokio::test]
    async fn feature_request_draft_parses_valid_json() {
        let valid_json = serde_json::json!({
            "problem_statement": "Users cannot export invoices in bulk",
            "proposed_value": "Add a bulk export feature supporting CSV and PDF formats",
            "affected_module": "Reporting",
            "priority": "medium",
            "acceptance_criteria": [
                "User can select multiple invoices for export",
                "Export completes within 30 seconds for up to 1000 invoices",
                "CSV and PDF formats are supported"
            ]
        })
        .to_string();

        let provider = Arc::new(FakeAiProvider::new().with_response_text(valid_json));
        let agent = test_agent(provider.clone());

        let request = feature_request_request(
            "We need to export many invoices at once instead of one by one.",
        );
        let ctx = synthetic_context();

        let draft = agent
            .generate_feature_request_draft_with_context(request, &ctx)
            .await
            .expect("draft generation should succeed");

        assert_eq!(
            draft.problem_statement,
            "Users cannot export invoices in bulk"
        );
        assert_eq!(
            draft.proposed_value,
            "Add a bulk export feature supporting CSV and PDF formats"
        );
        assert_eq!(draft.affected_module, "Reporting");
        assert_eq!(draft.priority, FeatureRequestPriority::Medium);
        assert_eq!(draft.acceptance_criteria.len(), 3);
        assert_eq!(
            draft.acceptance_criteria[0],
            "User can select multiple invoices for export"
        );

        // Verify the provider received the prompt with feature-request-specific instructions
        let requests = provider.take_requests();
        assert_eq!(requests.len(), 1);
        let system_msg = &requests[0].messages[0];
        assert!(system_msg.content.contains("problem_statement"));
        assert!(system_msg.content.contains("proposed_value"));
        assert!(system_msg.content.contains("affected_module"));
        assert!(system_msg.content.contains("priority"));
        assert!(system_msg.content.contains("acceptance_criteria"));
        // Lower temperature for structured output
        assert_eq!(requests[0].temperature, Some(0.3));
    }

    /// Proves that generate_feature_request_draft returns an error when the
    /// provider returns malformed JSON.
    #[tokio::test]
    async fn feature_request_draft_rejects_malformed_json() {
        let provider = Arc::new(
            FakeAiProvider::new()
                .with_response_text("This is not JSON, just freeform text about a feature."),
        );
        let agent = test_agent(provider);

        let request = feature_request_request("We need a new dashboard.");
        let ctx = synthetic_context();

        let result = agent
            .generate_feature_request_draft_with_context(request, &ctx)
            .await;

        assert!(result.is_err(), "should fail for malformed JSON");
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Failed to parse feature request draft"),
            "error message should mention parse failure, got: {}",
            err
        );
    }

    /// Proves that generate_feature_request_draft handles JSON wrapped in
    /// markdown code fences (common provider behavior).
    #[tokio::test]
    async fn feature_request_draft_strips_markdown_fences() {
        let raw_json = serde_json::json!({
            "problem_statement": "No audit trail for approval overrides",
            "proposed_value": "Log all override actions with reason codes",
            "affected_module": "Workflows",
            "priority": "high",
            "acceptance_criteria": ["Override reason is mandatory", "Audit log entry is created"]
        })
        .to_string();

        let fenced = format!("```json\n{}\n```", raw_json);

        let provider = Arc::new(FakeAiProvider::new().with_response_text(fenced));
        let agent = test_agent(provider);

        let request = feature_request_request("Need to track approval overrides");
        let ctx = synthetic_context();

        let draft = agent
            .generate_feature_request_draft_with_context(request, &ctx)
            .await
            .expect("should strip fences and parse");

        assert_eq!(
            draft.problem_statement,
            "No audit trail for approval overrides"
        );
        assert_eq!(draft.priority, FeatureRequestPriority::High);
    }

    // -------------------------------------------------------------------------
    // Source metadata provenance tests
    // -------------------------------------------------------------------------

    /// Proves that a bug report draft includes source metadata with the
    /// conversation id, deterministic link, intake channel, and issue kind
    /// when a conversation_id is provided.
    #[tokio::test]
    async fn bug_report_draft_includes_source_metadata_with_conversation() {
        let conv_id = Uuid::new_v4();
        let valid_json = serde_json::json!({
            "title": "Crash",
            "current_behavior": "Crashes",
            "expected_behavior": "Does not crash",
            "reproduction_steps": ["Open app"],
            "priority": "high",
            "affected_module": "Auth",
            "acceptance_criteria": ["No crash"]
        })
        .to_string();

        let provider = Arc::new(FakeAiProvider::new().with_response_text(valid_json));
        let agent = test_agent(provider);

        let request = BugReportDraftRequest {
            description: "It crashes.".to_string(),
            conversation_id: Some(conv_id),
        };
        let ctx = synthetic_context();

        let draft = agent
            .generate_bug_report_draft_with_context(request, &ctx)
            .await
            .expect("draft generation should succeed");

        let meta = draft.metadata.as_ref().expect("metadata must be present");
        assert_eq!(meta.source_conversation_id, Some(conv_id));
        assert_eq!(
            meta.source_conversation_link,
            Some(format!("/ai-assistant?conversation_id={}", conv_id))
        );
        assert_eq!(meta.intake_channel, "winston_ai");
        assert_eq!(meta.issue_kind, "bug");
    }

    /// Proves that a feature request draft includes source metadata with the
    /// conversation id, deterministic link, intake channel, and issue kind
    /// when a conversation_id is provided.
    #[tokio::test]
    async fn feature_request_draft_includes_source_metadata_with_conversation() {
        let conv_id = Uuid::new_v4();
        let valid_json = serde_json::json!({
            "problem_statement": "Need export",
            "proposed_value": "Add CSV export",
            "affected_module": "Reporting",
            "priority": "medium",
            "acceptance_criteria": ["Exports CSV"]
        })
        .to_string();

        let provider = Arc::new(FakeAiProvider::new().with_response_text(valid_json));
        let agent = test_agent(provider);

        let request = FeatureRequestDraftRequest {
            description: "We need export.".to_string(),
            conversation_id: Some(conv_id),
        };
        let ctx = synthetic_context();

        let draft = agent
            .generate_feature_request_draft_with_context(request, &ctx)
            .await
            .expect("draft generation should succeed");

        let meta = draft.metadata.as_ref().expect("metadata must be present");
        assert_eq!(meta.source_conversation_id, Some(conv_id));
        assert_eq!(
            meta.source_conversation_link,
            Some(format!("/ai-assistant?conversation_id={}", conv_id))
        );
        assert_eq!(meta.intake_channel, "winston_ai");
        assert_eq!(meta.issue_kind, "feature_request");
    }

    /// Proves that metadata is still populated (with None conversation fields)
    /// when no conversation_id is provided.
    #[tokio::test]
    async fn bug_report_draft_metadata_present_without_conversation() {
        let valid_json = serde_json::json!({
            "title": "Bug",
            "current_behavior": "Broken",
            "expected_behavior": "Fixed",
            "reproduction_steps": ["Do thing"],
            "priority": "low",
            "affected_module": "Core",
            "acceptance_criteria": ["Works"]
        })
        .to_string();

        let provider = Arc::new(FakeAiProvider::new().with_response_text(valid_json));
        let agent = test_agent(provider);

        let request = BugReportDraftRequest {
            description: "Something.".to_string(),
            conversation_id: None,
        };
        let ctx = synthetic_context();

        let draft = agent
            .generate_bug_report_draft_with_context(request, &ctx)
            .await
            .expect("draft generation should succeed");

        let meta = draft.metadata.as_ref().expect("metadata must be present");
        assert_eq!(meta.source_conversation_id, None);
        assert_eq!(meta.source_conversation_link, None);
        assert_eq!(meta.intake_channel, "winston_ai");
        assert_eq!(meta.issue_kind, "bug");
    }

    // -------------------------------------------------------------------------
    // Agentic loop iteration tests
    // -------------------------------------------------------------------------

    /// Proves the agent iterates over tool calls across multiple hops until the
    /// model produces a final text-only answer.
    #[tokio::test]
    async fn agent_iterates_tool_calls_until_final_answer() {
        let tool_call_response = ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: String::new(),
            },
            tool_calls: Some(vec![ProviderToolCall {
                id: Some("call-001".to_string()),
                name: "get_module_capabilities".to_string(),
                arguments: json!({}),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: None,
            provider_request_id: Some("fake-req-001".to_string()),
        };
        let final_text_response = ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: "the final answer".to_string(),
            },
            tool_calls: None,
            finish_reason: Some("stop".to_string()),
            usage: None,
            provider_request_id: Some("fake-req-002".to_string()),
        };

        let provider = Arc::new(
            FakeAiProvider::new()
                .with_tools_supported(true)
                .with_response_sequence(vec![
                    tool_call_response.clone(),
                    tool_call_response.clone(),
                    tool_call_response.clone(),
                    final_text_response,
                ]),
        );
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("Tell me about modules");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx, conversation_id)
            .await
            .expect("chat_with_context should succeed");

        // 3 tool-call responses + 1 final text = 4 total provider requests
        let requests = provider.take_requests();
        assert_eq!(
            requests.len(),
            4,
            "expected 4 provider requests (3 tool + 1 final)"
        );

        // 3 tool traces (one per tool execution)
        assert_eq!(response.trace.tools_used.len(), 3);
        for trace in &response.trace.tools_used {
            assert_eq!(trace.tool_name, "get_module_capabilities");
        }

        // Final content is the terminating text
        assert_eq!(response.message.content, "the final answer");

        // Every intermediate request must carry tools
        for (i, req) in requests.iter().enumerate() {
            assert!(
                req.tools.is_some(),
                "request {} should carry tools for agentic iteration",
                i
            );
        }
    }

    /// Proves the agentic loop stops at MAX_TOOL_ITERATIONS and forces a final
    /// text-only completion with tools: None.
    #[tokio::test]
    async fn agent_stops_at_max_tool_iterations() {
        let tool_call_response = ProviderChatResponse {
            message: ProviderChatMessage {
                role: ProviderMessageRole::Assistant,
                content: String::new(),
            },
            tool_calls: Some(vec![ProviderToolCall {
                id: Some("call-001".to_string()),
                name: "get_module_capabilities".to_string(),
                arguments: json!({}),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: None,
            provider_request_id: Some("fake-req-001".to_string()),
        };

        // 6 scripted tool-call responses: 1 initial + 5 loop iterations
        let mut sequence: Vec<ProviderChatResponse> = Vec::new();
        for _ in 0..6 {
            sequence.push(tool_call_response.clone());
        }
        let provider = Arc::new(
            FakeAiProvider::new()
                .with_tools_supported(true)
                .with_response_text("forced terminator answer")
                .with_response_sequence(sequence),
        );
        let agent = test_agent(provider.clone());

        let ctx = synthetic_context();
        let request = chat_request("Keep using tools");
        let conversation_id = Uuid::new_v4();

        let response = agent
            .chat_with_context(request, ctx, conversation_id)
            .await
            .expect("chat_with_context should succeed");

        let requests = provider.take_requests();
        // 6 scripted calls (initial + 5 loop iterations) + 1 terminator = 7 total
        assert_eq!(
            requests.len(),
            7,
            "expected 7 provider requests (6 loop + 1 terminator), got {}",
            requests.len()
        );

        // The final request must have tools: None (the terminator)
        let last_request = requests.last().expect("should have a last request");
        assert!(
            last_request.tools.is_none(),
            "terminator request must have tools: None"
        );

        // All prior requests must have tools
        for (i, req) in requests.iter().enumerate().take(requests.len() - 1) {
            assert!(
                req.tools.is_some(),
                "non-terminator request {} should carry tools",
                i
            );
        }

        // The response content should be from the forced terminator
        assert!(
            !response.message.content.is_empty(),
            "response should have content from the forced terminator"
        );
    }
}
