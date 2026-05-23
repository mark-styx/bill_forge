//! Tests for the agentic loop wiring: tool descriptions and argument extraction.
//!
//! These run without a database or OpenAI key (SQLX_OFFLINE=true).

use billforge_ai_agent::tools::ToolRegistry;
use billforge_ai_agent::tools::{AiToolDefinition, AiToolClass, AiToolPermission, AiToolRiskLevel};
use sqlx::postgres::PgPoolOptions;

fn fake_pool() -> sqlx::PgPool {
    PgPoolOptions::new()
        .connect_lazy("postgres://fake")
        .expect("lazy connection should not fail")
}

#[tokio::test]
async fn test_tool_descriptions_cover_all_registered_tools() {
    let registry = ToolRegistry::new(fake_pool());
    let descriptions = registry.get_tool_descriptions();

    assert!(descriptions.contains("get_invoice_status"), "missing get_invoice_status");
    assert!(descriptions.contains("get_vendor_invoices"), "missing get_vendor_invoices");
    assert!(descriptions.contains("get_approval_requirements"), "missing get_approval_requirements");
    assert!(descriptions.contains("summarize_invoice"), "missing summarize_invoice");
    assert!(descriptions.contains("get_module_capabilities"), "missing get_module_capabilities");
    assert!(descriptions.contains("search_product_docs"), "missing search_product_docs");
    assert!(descriptions.contains("explain_feature"), "missing explain_feature");
    assert!(descriptions.contains("search_known_issues"), "missing search_known_issues");
    assert!(descriptions.contains("summarize_release_changes"), "missing summarize_release_changes");
    assert!(descriptions.contains("explain_workflow_behavior"), "missing explain_workflow_behavior");
}

/// Validate primary-argument extraction logic inline (mirrors what
/// the agent loop does when parsing tool call JSON).
#[test]
fn test_extract_primary_arg_for_each_tool() {
    let uuid = "550e8400-e29b-41d4-a716-446655440000";

    // Invoice-id tools
    let json = format!(r#"{{"invoice_id":"{}"}}"#, uuid);
    for tool in &["get_invoice_status", "get_approval_requirements", "summarize_invoice"] {
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let result = parsed["invoice_id"].as_str().unwrap();
        assert_eq!(result, uuid, "failed for tool {}", tool);
    }

    // Vendor-name tool
    let vendor_json = r#"{"vendor_name":"Acme Corp"}"#;
    let parsed: serde_json::Value = serde_json::from_str(vendor_json).unwrap();
    let result = parsed["vendor_name"].as_str().unwrap();
    assert_eq!(result, "Acme Corp");

    // Invalid JSON → error
    assert!(serde_json::from_str::<serde_json::Value>("not json").is_err());

    // Product docs tools - query arg
    let query_json = r#"{"query":"invoice processing"}"#;
    let parsed: serde_json::Value = serde_json::from_str(query_json).unwrap();
    let result = parsed["query"].as_str().unwrap();
    assert_eq!(result, "invoice processing");

    // Explain feature tool - feature arg
    let feature_json = r#"{"feature":"vendor management"}"#;
    let parsed: serde_json::Value = serde_json::from_str(feature_json).unwrap();
    let result = parsed["feature"].as_str().unwrap();
    assert_eq!(result, "vendor management");

    // Known issues tool - query arg
    let ki_json = r#"{"query":"login timeout"}"#;
    let parsed: serde_json::Value = serde_json::from_str(ki_json).unwrap();
    let result = parsed["query"].as_str().unwrap();
    assert_eq!(result, "login timeout");

    // Release changes tool - optional query/version arg
    let rc_json = r#"{"query":"2026-03-20"}"#;
    let parsed: serde_json::Value = serde_json::from_str(rc_json).unwrap();
    let result = parsed["query"].as_str().unwrap();
    assert_eq!(result, "2026-03-20");
    let rc_version_json = r#"{"version":"2026-03-20"}"#;
    let parsed: serde_json::Value = serde_json::from_str(rc_version_json).unwrap();
    let result = parsed["version"].as_str().unwrap();
    assert_eq!(result, "2026-03-20");
}

/// Build a fake AgentContext with selected enabled modules.
fn agent_context_with_modules(modules: Vec<billforge_core::Module>) -> billforge_ai_agent::models::AgentContext {
    billforge_ai_agent::models::AgentContext {
        tenant_id: "00000000-0000-0000-0000-000000000001".to_string(),
        user_id: uuid::Uuid::new_v4(),
        user_role: "admin".to_string(),
        permissions: vec!["read".to_string()],
        enabled_modules: modules,
    }
}

#[tokio::test]
async fn test_get_module_capabilities_enabled_modules_marked_enabled() {
    let ctx = agent_context_with_modules(vec![
        billforge_core::Module::InvoiceCapture,
        billforge_core::Module::AiAssistant,
    ]);

    let registry = ToolRegistry::new(fake_pool());
    let result = registry
        .execute_tool("get_module_capabilities", &ctx, "")
        .await
        .expect("tool should succeed");

    // Enabled modules should be marked ENABLED
    assert!(
        result.contains("Invoice Capture (invoice_capture): ENABLED"),
        "Invoice Capture should be ENABLED, got: {result}"
    );
    assert!(
        result.contains("Winston AI Assistant (ai_assistant): ENABLED"),
        "AiAssistant should be ENABLED, got: {result}"
    );

    // Disabled modules should be marked DISABLED
    assert!(
        result.contains("Invoice Processing (invoice_processing): DISABLED"),
        "Invoice Processing should be DISABLED, got: {result}"
    );
    assert!(
        result.contains("Vendor Management (vendor_management): DISABLED"),
        "Vendor Management should be DISABLED, got: {result}"
    );
    assert!(
        result.contains("Reporting & Analytics (reporting): DISABLED"),
        "Reporting should be DISABLED, got: {result}"
    );

    // Disabled modules should include boundary language
    assert!(
        result.contains("not available for this organization"),
        "disabled modules should include boundary language, got: {result}"
    );
}

#[tokio::test]
async fn test_get_module_capabilities_ai_assistant_disabled_when_omitted() {
    let ctx = agent_context_with_modules(vec![
        billforge_core::Module::InvoiceCapture,
        billforge_core::Module::InvoiceProcessing,
    ]);

    let registry = ToolRegistry::new(fake_pool());
    let result = registry
        .execute_tool("get_module_capabilities", &ctx, "")
        .await
        .expect("tool should succeed");

    // AiAssistant should be DISABLED when not in enabled_modules
    assert!(
        result.contains("Winston AI Assistant (ai_assistant): DISABLED"),
        "AiAssistant should be DISABLED when omitted, got: {result}"
    );
    assert!(
        result.contains("paid add-on"),
        "AiAssistant disabled boundary should mention paid add-on, got: {result}"
    );

    // Enabled modules should still be ENABLED
    assert!(
        result.contains("Invoice Capture (invoice_capture): ENABLED"),
        "Invoice Capture should be ENABLED, got: {result}"
    );
    assert!(
        result.contains("Invoice Processing (invoice_processing): ENABLED"),
        "Invoice Processing should be ENABLED, got: {result}"
    );
}

#[tokio::test]
async fn test_search_product_docs_returns_source_references() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("search_product_docs", &ctx, "invoice processing")
        .await
        .expect("search_product_docs should succeed");

    // Should contain source references (square-bracket paths)
    assert!(
        result.contains('[') || result.contains("docs/northstar.md") || result.contains("CHANGELOG.md"),
        "expected source references in response, got: {result}"
    );

    // Should contain at least one known indexed path
    let has_known_path = result.contains("docs/northstar.md")
        || result.contains("CHANGELOG.md")
        || result.contains(".github/workflows/release.yml");
    assert!(
        has_known_path,
        "expected at least one known indexed path, got: {result}"
    );
}

#[tokio::test]
async fn test_search_product_docs_empty_query_returns_message() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("search_product_docs", &ctx, "  ")
        .await
        .expect("search_product_docs should succeed");

    assert!(
        result.contains("Please provide a search query"),
        "empty query should return helpful message, got: {result}"
    );
}

#[tokio::test]
async fn test_explain_feature_returns_explanation_with_sources() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("explain_feature", &ctx, "changelog")
        .await
        .expect("explain_feature should succeed");

    // Response should be explanation-oriented
    assert!(
        result.contains("Explanation for"),
        "expected explanation header, got: {result}"
    );

    // Response should include source references
    assert!(
        result.contains("from "),
        "expected source references, got: {result}"
    );

    // Response should note it is documentation-grounded
    assert!(
        result.contains("indexed product documentation"),
        "expected documentation grounding note, got: {result}"
    );
}

#[tokio::test]
async fn test_explain_feature_empty_input_returns_message() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("explain_feature", &ctx, "")
        .await
        .expect("explain_feature should succeed");

    assert!(
        result.contains("Please provide a feature name"),
        "empty input should return helpful message, got: {result}"
    );
}

/// Validate invoice_id extraction for explain_workflow_behavior:
/// JSON {"invoice_id":"<uuid>"} and raw UUID.
#[test]
fn test_extract_invoice_id_for_explain_workflow_behavior() {
    let uuid = "550e8400-e29b-41d4-a716-446655440000";

    // JSON arg extraction
    let json = format!(r#"{{"invoice_id":"{}"}}"#, uuid);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["invoice_id"].as_str().unwrap();
    assert_eq!(result, uuid, "JSON extraction failed for explain_workflow_behavior");

    // Raw UUID parses correctly
    let parsed_uuid: uuid::Uuid = uuid.parse().unwrap();
    assert_eq!(parsed_uuid.to_string(), uuid, "raw UUID parse failed");
}

#[tokio::test]
async fn test_explain_workflow_behavior_empty_input_returns_message() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("explain_workflow_behavior", &ctx, "")
        .await;
    // Should return an error with helpful message (not panic)
    assert!(result.is_err(), "empty input should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Please provide an invoice_id"),
        "empty input should mention invoice_id, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_explain_workflow_behavior_invalid_uuid_returns_message() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("explain_workflow_behavior", &ctx, "not-a-uuid")
        .await;
    assert!(result.is_err(), "invalid UUID should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid invoice ID format") || err_msg.contains("Invalid"),
        "invalid UUID should mention format, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_explain_workflow_behavior_invalid_json_returns_message() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("explain_workflow_behavior", &ctx, "{bad json}")
        .await;
    assert!(result.is_err(), "invalid JSON should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid JSON") || err_msg.contains("Invalid"),
        "invalid JSON should mention format, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_search_known_issues_returns_known_issue_source_references() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("search_known_issues", &ctx, "known issues")
        .await
        .expect("search_known_issues should succeed");

    // Should contain known issues source path
    assert!(
        result.contains("docs/known_issues.md"),
        "expected docs/known_issues.md in response, got: {result}"
    );
}

#[tokio::test]
async fn test_search_known_issues_empty_query_returns_message() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("search_known_issues", &ctx, "  ")
        .await
        .expect("search_known_issues should succeed");

    assert!(
        result.contains("Please provide a search query"),
        "empty query should return helpful message, got: {result}"
    );
}

#[tokio::test]
async fn test_summarize_release_changes_returns_changelog_source_references() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("summarize_release_changes", &ctx, "CI pipeline fixes")
        .await
        .expect("summarize_release_changes should succeed");

    // Should contain CHANGELOG.md source reference
    assert!(
        result.contains("CHANGELOG.md"),
        "expected CHANGELOG.md in response, got: {result}"
    );

    // Should contain release-summary header
    assert!(
        result.contains("Release Summary"),
        "expected Release Summary header, got: {result}"
    );

    // Should contain source-grounding note
    assert!(
        result.contains("indexed release notes"),
        "expected source-grounding note, got: {result}"
    );
}

#[tokio::test]
async fn test_summarize_release_changes_empty_input_still_returns_release_summary() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("summarize_release_changes", &ctx, "")
        .await
        .expect("summarize_release_changes should succeed with empty input");

    // Empty input uses default query, should still return results
    assert!(
        result.contains("Release Summary"),
        "expected Release Summary header even with empty input, got: {result}"
    );

    // Should contain CHANGELOG.md reference
    assert!(
        result.contains("CHANGELOG.md"),
        "expected CHANGELOG.md in response with empty input, got: {result}"
    );
}

#[tokio::test]
async fn test_tool_descriptions_include_request_issue_creation() {
    let registry = ToolRegistry::new(fake_pool());
    let descriptions = registry.get_tool_descriptions();

    assert!(
        descriptions.contains("request_issue_creation"),
        "missing request_issue_creation, got: {descriptions}"
    );
}

#[tokio::test]
async fn test_request_issue_creation_github_returns_approval_required() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let args = serde_json::json!({
        "target": "github",
        "kind": "bug",
        "title": "Login button not working",
        "body": "Clicking the login button on the main page results in a blank screen.",
        "labels": ["bug", "ui"],
        "source_conversation_id": "conv-123"
    })
    .to_string();

    let result = registry
        .execute_tool("request_issue_creation", &ctx, &args)
        .await
        .expect("request_issue_creation should succeed");

    let parsed: serde_json::Value = serde_json::from_str(&result)
        .expect("tool output should be valid JSON");

    // Status must be approval_required
    assert_eq!(
        parsed["status"].as_str(),
        Some("approval_required"),
        "status should be approval_required, got: {result}"
    );

    // Target preserved
    assert_eq!(
        parsed["request"]["target"].as_str(),
        Some("github"),
        "target should be github, got: {result}"
    );

    // Title preserved
    assert_eq!(
        parsed["request"]["title"].as_str(),
        Some("Login button not working"),
        "title should be preserved, got: {result}"
    );

    // Must include approval_request_id (non-nil UUID)
    let approval_id = parsed["approval_request_id"]
        .as_str()
        .expect("approval_request_id should be present");
    assert!(
        uuid::Uuid::parse_str(approval_id).is_ok(),
        "approval_request_id should be a valid UUID, got: {approval_id}"
    );

    // Must contain messaging that no issue was created
    assert!(
        result.contains("No external issue"),
        "response should state no external issue was created, got: {result}"
    );
}

#[tokio::test]
async fn test_request_issue_creation_empty_title_fails() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let args = serde_json::json!({
        "target": "github",
        "kind": "bug",
        "title": "  ",
        "body": "Valid body text"
    })
    .to_string();

    let result = registry
        .execute_tool("request_issue_creation", &ctx, &args)
        .await;

    assert!(result.is_err(), "empty title should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Title must not be empty"),
        "empty title error should mention title, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_request_issue_creation_unsupported_target_fails() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    // The target deserialization will fail for unsupported enum variants
    let args = serde_json::json!({
        "target": "bitbucket",
        "kind": "bug",
        "title": "A title",
        "body": "A body"
    })
    .to_string();

    let result = registry
        .execute_tool("request_issue_creation", &ctx, &args)
        .await;

    assert!(result.is_err(), "unsupported target should fail");
}

// ── Typed tool registry tests ─────────────────────────────────────────────────

/// Every tool exposed in descriptions must have exactly one typed definition.
#[tokio::test]
async fn test_typed_tool_registry_covers_all_tool_descriptions() {
    let registry = ToolRegistry::new(fake_pool());
    let descriptions = registry.get_tool_descriptions();
    let definitions = ToolRegistry::tool_definitions();

    // Extract tool names from descriptions (lines starting with "- ")
    let desc_names: Vec<&str> = descriptions
        .lines()
        .filter(|l| l.starts_with("- "))
        .map(|l| l[2..].split_once(':').map(|(n, _)| n.trim()).unwrap_or(l))
        .collect();

    assert!(!desc_names.is_empty(), "descriptions should not be empty");

    // Each description name must have exactly one definition
    for name in &desc_names {
        let matches: Vec<&AiToolDefinition> =
            definitions.iter().filter(|d| d.name == *name).collect();
        assert_eq!(
            matches.len(),
            1,
            "tool '{}' should have exactly 1 definition, found {}",
            name,
            matches.len()
        );
    }

    // Each definition must appear in descriptions
    for def in &definitions {
        assert!(
            descriptions.contains(def.name),
            "definition '{}' missing from descriptions",
            def.name
        );
    }

    // Counts must match
    assert_eq!(
        desc_names.len(),
        definitions.len(),
        "description count ({}) != definition count ({})",
        desc_names.len(),
        definitions.len()
    );
}

/// Every typed definition must have non-empty metadata fields.
#[test]
fn test_typed_tool_definitions_have_non_empty_fields() {
    for def in ToolRegistry::tool_definitions() {
        assert!(
            !def.name.is_empty(),
            "tool definition has empty name"
        );
        assert!(
            !def.description.is_empty(),
            "tool '{}' has empty description",
            def.name
        );
        assert!(
            def.input_schema.is_object(),
            "tool '{}' input_schema is not an object",
            def.name
        );
        assert!(
            !def.input_schema.as_object().unwrap().is_empty(),
            "tool '{}' has empty input_schema",
            def.name
        );
        assert!(
            def.output_schema.is_object(),
            "tool '{}' output_schema is not an object",
            def.name
        );
        assert!(
            !def.output_schema.as_object().unwrap().is_empty(),
            "tool '{}' has empty output_schema",
            def.name
        );
        // Ensure concrete risk level (not a default-like sentinel)
        assert!(
            matches!(def.risk_level, AiToolRiskLevel::Low | AiToolRiskLevel::Medium | AiToolRiskLevel::High),
            "tool '{}' has unexpected risk level",
            def.name
        );
    }
}

/// Spot-check: get_invoice_status
#[test]
fn test_typed_definition_get_invoice_status() {
    let def = ToolRegistry::get_tool_definition("get_invoice_status")
        .expect("get_invoice_status should have a definition");

    assert_eq!(def.class, AiToolClass::Invoice);
    assert_eq!(def.required_permission, AiToolPermission::InvoiceRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def.input_schema
        .get("required")
        .and_then(|r| r.as_array())
        .expect("input_schema should have required array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        required_names.contains(&"invoice_id"),
        "input_schema should require invoice_id, got: {:?}",
        required_names
    );
}

/// Spot-check: get_vendor_invoices
#[test]
fn test_typed_definition_get_vendor_invoices() {
    let def = ToolRegistry::get_tool_definition("get_vendor_invoices")
        .expect("get_vendor_invoices should have a definition");

    assert_eq!(def.class, AiToolClass::Vendor);
    assert_eq!(def.required_permission, AiToolPermission::VendorRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def.input_schema
        .get("required")
        .and_then(|r| r.as_array())
        .expect("input_schema should have required array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        required_names.contains(&"vendor_name"),
        "input_schema should require vendor_name, got: {:?}",
        required_names
    );
}

/// Spot-check: request_issue_creation
#[test]
fn test_typed_definition_request_issue_creation() {
    let def = ToolRegistry::get_tool_definition("request_issue_creation")
        .expect("request_issue_creation should have a definition");

    assert_eq!(def.class, AiToolClass::IssueIntake);
    assert_eq!(def.required_permission, AiToolPermission::IssueRequest);
    assert_eq!(def.risk_level, AiToolRiskLevel::Medium);
    assert!(!def.mutates, "request_issue_creation should NOT be marked as mutates");

    let required = def.input_schema
        .get("required")
        .and_then(|r| r.as_array())
        .expect("input_schema should have required array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
    for field in &["target", "kind", "title", "body"] {
        assert!(
            required_names.contains(field),
            "input_schema should require '{}', got: {:?}",
            field,
            required_names
        );
    }
}

/// All tools should have mutates == false in this slice.
#[test]
fn test_all_tools_are_non_mutating() {
    for def in ToolRegistry::tool_definitions() {
        assert!(
            !def.mutates,
            "tool '{}' should be mutates: false in this slice",
            def.name
        );
    }
}

/// get_tool_definition returns None for unknown tools.
#[test]
fn test_get_tool_definition_returns_none_for_unknown() {
    assert!(
        ToolRegistry::get_tool_definition("nonexistent_tool").is_none(),
        "unknown tool should return None"
    );
}
