//! Tests for the agentic loop wiring: tool descriptions and argument extraction.
//!
//! These run without a database or OpenAI key (SQLX_OFFLINE=true).

use billforge_ai_agent::tools::ToolRegistry;
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
