# Winston Tool Execution Audit

This audit identifies every current code path that can directly invoke a
Winston tool and records the present mutating/high-impact tool posture.

## Direct Tool Execution Surface

### Production API routes

The production API entrypoints live in `backend/crates/api/src/routes/ai.rs`.

- `POST /ai/chat` is handled by `chat_handler`, constructs a `WinstonAgent`,
  and calls `WinstonAgent::chat(...)`.
- `POST /ai/conversations/{id}/messages` is handled by
  `continue_conversation_handler`, attaches the route conversation id to the
  `ChatRequest`, constructs a `WinstonAgent`, and calls
  `WinstonAgent::chat(...)`.

These are production chat entrypoints. They are tool-capable only through the
shared `WinstonAgent::chat` provider turn described below.

### Legacy ai-agent handlers

The legacy ai-agent HTTP handlers live in
`backend/crates/ai-agent/src/handlers.rs`.

- `POST /api/ai/chat` is handled by `chat_handler` and calls
  `state.agent.chat(...)`.
- `POST /api/ai/conversations/:id/messages` is handled by
  `continue_conversation_handler`, attaches the route conversation id to the
  `ChatRequest`, and calls `state.agent.chat(...)`.

These legacy handlers also enter the same shared `WinstonAgent::chat` path.

### Shared chat path

`WinstonAgent::chat(...)` in `backend/crates/ai-agent/src/agent.rs` injects
authenticated tenant/user context, persists the user message, and calls
`execute_provider_turn(...)`.

`execute_provider_turn(...)` is the only audited chat path that advertises
tools to providers. When `self.provider.supports_tools()` is true, it sets the
provider request `tools` field to
`Some(self.tools.provider_tool_definitions())`. If the provider returns tool
calls, `execute_provider_turn(...)` loops over those calls and directly invokes:

```rust
self.tools.execute_tool(&call.name, context, &args).await
```

The follow-up provider request after tool execution sets `tools: None`, so tool
results are summarized without advertising tools again in that follow-up turn.

### Central dispatcher

`ToolRegistry::execute_tool(...)` in `backend/crates/ai-agent/src/tools.rs` is
the single dispatcher capable of executing any registered tool name. It first
rejects unknown tool names through `ToolRegistry::get_tool_definition(...)`,
then dispatches by `match tool_name` to the concrete tool implementation.

Any future mutating or high-impact registered tool exposed through
`ToolRegistry::provider_tool_definitions()` would currently be invokable by the
shared chat path when returned by a tool-capable provider.

## Non-Tool Draft Paths

The bug-report and feature-request draft paths in
`backend/crates/ai-agent/src/agent.rs` are not direct tool execution surfaces.

- `generate_bug_report_draft(...)` delegates to
  `generate_bug_report_draft_with_context(...)`, whose provider request sets
  `tools: None`.
- `generate_feature_request_draft(...)` delegates to
  `generate_feature_request_draft_with_context(...)`, whose provider request
  sets `tools: None`.

These paths call the provider for JSON draft generation only. They do not pass
provider tool definitions and do not invoke `ToolRegistry::execute_tool(...)`.

## Current Registry Risk Finding

The authoritative tool metadata is `ToolRegistry::tool_definitions()`.

Current findings:

- No registered tool has `mutates: true`.
- No registered tool has `AiToolRiskLevel::High`.
- `request_issue_creation` is `AiToolRiskLevel::Medium` and has
  `mutates: false`. Its dispatcher branch calls
  `prepare_issue_creation_for_approval(...)` and returns an approval envelope;
  it does not create an external GitHub, Linear, Jira, or internal feedback
  issue.

Focused regression tests in `agentic_loop_tests.rs` pin these findings for both
provider-exposed tools and the full typed registry.
