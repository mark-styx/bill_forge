//! Tests for the agentic loop wiring: tool descriptions and argument extraction.
//!
//! These run without a database or OpenAI key (SQLX_OFFLINE=true).

use billforge_ai_agent::tools::ToolRegistry;
use billforge_ai_agent::tools::{AiToolClass, AiToolDefinition, AiToolPermission, AiToolRiskLevel};
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

    assert!(
        descriptions.contains("get_invoice_status"),
        "missing get_invoice_status"
    );
    assert!(
        descriptions.contains("get_vendor_invoices"),
        "missing get_vendor_invoices"
    );
    assert!(
        descriptions.contains("get_approval_requirements"),
        "missing get_approval_requirements"
    );
    assert!(
        descriptions.contains("summarize_invoice"),
        "missing summarize_invoice"
    );
    assert!(
        descriptions.contains("get_module_capabilities"),
        "missing get_module_capabilities"
    );
    assert!(
        descriptions.contains("search_product_docs"),
        "missing search_product_docs"
    );
    assert!(
        descriptions.contains("explain_feature"),
        "missing explain_feature"
    );
    assert!(
        descriptions.contains("search_known_issues"),
        "missing search_known_issues"
    );
    assert!(
        descriptions.contains("summarize_release_changes"),
        "missing summarize_release_changes"
    );
    assert!(
        descriptions.contains("explain_workflow_behavior"),
        "missing explain_workflow_behavior"
    );
    assert!(
        descriptions.contains("explain_workflow_state"),
        "missing explain_workflow_state"
    );
    assert!(
        descriptions.contains("search_invoices"),
        "missing search_invoices"
    );
    assert!(
        descriptions.contains("find_duplicate_invoice_candidates"),
        "missing find_duplicate_invoice_candidates"
    );
    assert!(
        descriptions.contains("assess_invoice_payment_risk"),
        "missing assess_invoice_payment_risk"
    );
    assert!(
        descriptions.contains("get_vendor_summary"),
        "missing get_vendor_summary"
    );
    for tool in [
        "get_tenant_usage_analysis",
        "get_workflow_bottlenecks",
        "get_rule_recommendations",
        "get_spend_analysis",
    ] {
        assert!(descriptions.contains(tool), "missing {tool}");
    }
}

/// Validate primary-argument extraction logic inline (mirrors what
/// the agent loop does when parsing tool call JSON).
#[test]
fn test_extract_primary_arg_for_each_tool() {
    let uuid = "550e8400-e29b-41d4-a716-446655440000";

    // Invoice-id tools
    let json = format!(r#"{{"invoice_id":"{}"}}"#, uuid);
    for tool in &[
        "get_invoice_status",
        "get_approval_requirements",
        "summarize_invoice",
        "explain_workflow_state",
    ] {
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let result = parsed["invoice_id"].as_str().unwrap();
        assert_eq!(result, uuid, "failed for tool {}", tool);
    }

    // Vendor-name tool
    let vendor_json = r#"{"vendor_name":"Acme Corp"}"#;
    let parsed: serde_json::Value = serde_json::from_str(vendor_json).unwrap();
    let result = parsed["vendor_name"].as_str().unwrap();
    assert_eq!(result, "Acme Corp");

    // Vendor summary tool - vendor_id arg
    let vendor_id_json = format!(r#"{{"vendor_id":"{}"}}"#, uuid);
    let parsed: serde_json::Value = serde_json::from_str(&vendor_id_json).unwrap();
    let result = parsed["vendor_id"].as_str().unwrap();
    assert_eq!(
        result, uuid,
        "vendor_id extraction failed for get_vendor_summary"
    );

    // Vendor summary tool - vendor_name arg
    let vn_json = r#"{"vendor_name":"Acme Corp"}"#;
    let parsed: serde_json::Value = serde_json::from_str(vn_json).unwrap();
    let result = parsed["vendor_name"].as_str().unwrap();
    assert_eq!(
        result, "Acme Corp",
        "vendor_name extraction failed for get_vendor_summary"
    );

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

#[tokio::test]
async fn test_explain_workflow_state_invalid_tenant_uuid_fails_before_db() {
    let mut ctx = agent_context_with_modules(vec![billforge_core::Module::AiAssistant]);
    ctx.tenant_id = "not-a-uuid".to_string();
    let registry = ToolRegistry::new(fake_pool());
    let result = registry
        .execute_tool(
            "explain_workflow_state",
            &ctx,
            "550e8400-e29b-41d4-a716-446655440000",
        )
        .await;

    let err = result.expect_err("invalid tenant UUID should fail");
    assert!(
        err.to_string().contains("Invalid tenant_id"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_admin_analysis_tools_reject_non_admin_before_db() {
    let mut ctx = agent_context_with_modules(vec![billforge_core::Module::AiAssistant]);
    ctx.user_role = "ap_user".to_string();
    ctx.permissions = vec!["read".to_string()];
    let registry = ToolRegistry::new(fake_pool());

    for tool in [
        "get_tenant_usage_analysis",
        "get_workflow_bottlenecks",
        "get_rule_recommendations",
        "get_spend_analysis",
    ] {
        let result = registry
            .execute_tool(tool, &ctx, r#"{"window_days":30}"#)
            .await;
        let err = result.expect_err("non-admin context should be rejected before DB access");
        let msg = err.to_string();
        assert!(
            msg.contains("admin-only") || msg.contains("Forbidden"),
            "unexpected error for {tool}: {msg}"
        );
    }
}

#[tokio::test]
async fn test_admin_analysis_provider_schemas_do_not_accept_tenant_id() {
    let registry = ToolRegistry::new(fake_pool());
    let provider_tools = registry.provider_tool_definitions();

    for tool_name in [
        "get_tenant_usage_analysis",
        "get_workflow_bottlenecks",
        "get_rule_recommendations",
        "get_spend_analysis",
    ] {
        let provider_tool = provider_tools
            .iter()
            .find(|tool| tool.name == tool_name)
            .unwrap_or_else(|| panic!("missing provider tool {tool_name}"));
        let properties = provider_tool
            .parameters
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("tool schema should have object properties");
        assert!(
            !properties.contains_key("tenant_id"),
            "{tool_name} must use authenticated tenant context, not model-provided tenant_id"
        );
    }
}

/// Build a fake AgentContext with selected enabled modules.
fn agent_context_with_modules(
    modules: Vec<billforge_core::Module>,
) -> billforge_ai_agent::models::AgentContext {
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
        result.contains('[')
            || result.contains("docs/northstar.md")
            || result.contains("CHANGELOG.md"),
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
    assert_eq!(
        result, uuid,
        "JSON extraction failed for explain_workflow_behavior"
    );

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

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("tool output should be valid JSON");

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
        assert!(!def.name.is_empty(), "tool definition has empty name");
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
            matches!(
                def.risk_level,
                AiToolRiskLevel::Low | AiToolRiskLevel::Medium | AiToolRiskLevel::High
            ),
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

    let required = def
        .input_schema
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

    let required = def
        .input_schema
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

/// Spot-check: get_approval_requirements (updated schema with approval_requests)
#[test]
fn test_typed_definition_get_approval_requirements() {
    let def = ToolRegistry::get_tool_definition("get_approval_requirements")
        .expect("get_approval_requirements should have a definition");

    assert_eq!(def.class, AiToolClass::Approval);
    assert_eq!(def.required_permission, AiToolPermission::ApprovalRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def
        .input_schema
        .get("required")
        .and_then(|r| r.as_array())
        .expect("input_schema should have required array");
    let required_names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        required_names.contains(&"invoice_id"),
        "input_schema should require invoice_id, got: {:?}",
        required_names
    );

    // Output schema should have approval_requests array
    let props = def
        .output_schema
        .get("properties")
        .expect("output_schema should have properties");
    assert!(
        props.get("approval_requests").is_some(),
        "output_schema should have approval_requests property"
    );
    assert!(
        props.get("invoice_status").is_some(),
        "output_schema should have invoice_status property"
    );
}

/// Spot-check: get_vendor_summary
#[test]
fn test_typed_definition_get_vendor_summary() {
    let def = ToolRegistry::get_tool_definition("get_vendor_summary")
        .expect("get_vendor_summary should have a definition");

    assert_eq!(def.class, AiToolClass::Vendor);
    assert_eq!(def.required_permission, AiToolPermission::VendorRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    // Should have vendor_id and vendor_name properties (not required)
    let props = def
        .input_schema
        .get("properties")
        .expect("input_schema should have properties");
    assert!(
        props.get("vendor_id").is_some(),
        "should have vendor_id property"
    );
    assert!(
        props.get("vendor_name").is_some(),
        "should have vendor_name property"
    );

    // Neither should be required
    let required = def.input_schema.get("required").and_then(|r| r.as_array());
    assert!(
        required.is_none() || required.unwrap().is_empty(),
        "get_vendor_summary should not require any specific field, got: {:?}",
        required
    );

    // Output schema should have expected properties
    let out_props = def
        .output_schema
        .get("properties")
        .expect("output_schema should have properties");
    assert!(
        out_props.get("vendor_id").is_some(),
        "output should have vendor_id"
    );
    assert!(
        out_props.get("vendor_name").is_some(),
        "output should have vendor_name"
    );
    assert!(
        out_props.get("is_active").is_some(),
        "output should have is_active"
    );
    assert!(
        out_props.get("contact_email").is_some(),
        "output should have contact_email"
    );
    assert!(
        out_props.get("total_invoices").is_some(),
        "output should have total_invoices"
    );
    assert!(
        out_props.get("recent_invoices").is_some(),
        "output should have recent_invoices"
    );
}

/// Spot-check: summarize_invoice
#[test]
fn test_typed_definition_summarize_invoice() {
    let def = ToolRegistry::get_tool_definition("summarize_invoice")
        .expect("summarize_invoice should have a definition");

    assert_eq!(def.class, AiToolClass::Invoice);
    assert_eq!(def.required_permission, AiToolPermission::InvoiceRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def
        .input_schema
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

/// Spot-check: explain_workflow_behavior
#[test]
fn test_typed_definition_explain_workflow_behavior() {
    let def = ToolRegistry::get_tool_definition("explain_workflow_behavior")
        .expect("explain_workflow_behavior should have a definition");

    assert_eq!(def.class, AiToolClass::Workflow);
    assert_eq!(def.required_permission, AiToolPermission::WorkflowRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def
        .input_schema
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

/// Spot-check: explain_workflow_state
#[test]
fn test_typed_definition_explain_workflow_state() {
    let def = ToolRegistry::get_tool_definition("explain_workflow_state")
        .expect("explain_workflow_state should have a definition");

    assert_eq!(def.class, AiToolClass::Workflow);
    assert_eq!(def.required_permission, AiToolPermission::WorkflowRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def
        .input_schema
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

/// Spot-check: request_issue_creation
#[test]
fn test_typed_definition_request_issue_creation() {
    let def = ToolRegistry::get_tool_definition("request_issue_creation")
        .expect("request_issue_creation should have a definition");

    assert_eq!(def.class, AiToolClass::IssueIntake);
    assert_eq!(def.required_permission, AiToolPermission::IssueRequest);
    assert_eq!(def.risk_level, AiToolRiskLevel::Medium);
    assert!(
        !def.mutates,
        "request_issue_creation should NOT be marked as mutates"
    );

    let required = def
        .input_schema
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

// ── New invoice tool tests ────────────────────────────────────────────────────

/// Spot-check: search_invoices typed definition
#[test]
fn test_typed_definition_search_invoices() {
    let def = ToolRegistry::get_tool_definition("search_invoices")
        .expect("search_invoices should have a definition");

    assert_eq!(def.class, AiToolClass::Invoice);
    assert_eq!(def.required_permission, AiToolPermission::InvoiceRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    // Should have properties for the filter fields
    let props = def
        .input_schema
        .get("properties")
        .expect("input_schema should have properties");
    assert!(
        props.get("query").is_some(),
        "search_invoices should have query property"
    );
    assert!(
        props.get("vendor_name").is_some(),
        "search_invoices should have vendor_name property"
    );
    assert!(
        props.get("status").is_some(),
        "search_invoices should have status property"
    );
    assert!(
        props.get("limit").is_some(),
        "search_invoices should have limit property"
    );
}

/// Spot-check: find_duplicate_invoice_candidates typed definition
#[test]
fn test_typed_definition_find_duplicate_invoice_candidates() {
    let def = ToolRegistry::get_tool_definition("find_duplicate_invoice_candidates")
        .expect("find_duplicate_invoice_candidates should have a definition");

    assert_eq!(def.class, AiToolClass::Invoice);
    assert_eq!(def.required_permission, AiToolPermission::InvoiceRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def
        .input_schema
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

/// Spot-check: assess_invoice_payment_risk typed definition
#[test]
fn test_typed_definition_assess_invoice_payment_risk() {
    let def = ToolRegistry::get_tool_definition("assess_invoice_payment_risk")
        .expect("assess_invoice_payment_risk should have a definition");

    assert_eq!(def.class, AiToolClass::Invoice);
    assert_eq!(def.required_permission, AiToolPermission::InvoiceRead);
    assert_eq!(def.risk_level, AiToolRiskLevel::Low);
    assert!(!def.mutates);

    let required = def
        .input_schema
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

/// Validate invoice_id extraction for find_duplicate_invoice_candidates:
/// JSON {"invoice_id":"<uuid>"} and raw UUID.
#[test]
fn test_extract_invoice_id_for_find_duplicate_invoice_candidates() {
    let uuid = "550e8400-e29b-41d4-a716-446655440000";

    // JSON arg extraction
    let json = format!(r#"{{"invoice_id":"{}"}}"#, uuid);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["invoice_id"].as_str().unwrap();
    assert_eq!(
        result, uuid,
        "JSON extraction failed for find_duplicate_invoice_candidates"
    );

    // Raw UUID parses correctly
    let parsed_uuid: uuid::Uuid = uuid.parse().unwrap();
    assert_eq!(parsed_uuid.to_string(), uuid, "raw UUID parse failed");
}

/// Validate invoice_id extraction for assess_invoice_payment_risk:
/// JSON {"invoice_id":"<uuid>"} and raw UUID.
#[test]
fn test_extract_invoice_id_for_assess_invoice_payment_risk() {
    let uuid = "550e8400-e29b-41d4-a716-446655440000";

    // JSON arg extraction
    let json = format!(r#"{{"invoice_id":"{}"}}"#, uuid);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let result = parsed["invoice_id"].as_str().unwrap();
    assert_eq!(
        result, uuid,
        "JSON extraction failed for assess_invoice_payment_risk"
    );

    // Raw UUID parses correctly
    let parsed_uuid: uuid::Uuid = uuid.parse().unwrap();
    assert_eq!(parsed_uuid.to_string(), uuid, "raw UUID parse failed");
}

/// search_invoices primary argument extraction:
/// JSON with query, vendor_name, status, etc.
#[test]
fn test_extract_primary_args_for_search_invoices() {
    let json = r#"{"vendor_name":"Acme Corp","status":"pending","limit":10}"#;
    let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(parsed["vendor_name"].as_str(), Some("Acme Corp"));
    assert_eq!(parsed["status"].as_str(), Some("pending"));
    assert_eq!(parsed["limit"].as_u64(), Some(10));

    // Also test raw text treated as query
    let raw = "some search text";
    let wrapped = serde_json::json!({ "query": raw });
    assert_eq!(wrapped["query"].as_str(), Some("some search text"));
}

/// find_duplicate_invoice_candidates with empty input returns error.
#[tokio::test]
async fn test_find_duplicate_invoice_candidates_empty_input_returns_error() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("find_duplicate_invoice_candidates", &ctx, "")
        .await;
    assert!(result.is_err(), "empty input should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Please provide an invoice_id"),
        "empty input should mention invoice_id, got: {err_msg}"
    );
}

/// find_duplicate_invoice_candidates with invalid UUID returns error.
#[tokio::test]
async fn test_find_duplicate_invoice_candidates_invalid_uuid_returns_error() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("find_duplicate_invoice_candidates", &ctx, "not-a-uuid")
        .await;
    assert!(result.is_err(), "invalid UUID should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid invoice ID format") || err_msg.contains("Invalid"),
        "invalid UUID should mention format, got: {err_msg}"
    );
}

/// assess_invoice_payment_risk with empty input returns error.
#[tokio::test]
async fn test_assess_invoice_payment_risk_empty_input_returns_error() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("assess_invoice_payment_risk", &ctx, "")
        .await;
    assert!(result.is_err(), "empty input should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Please provide an invoice_id"),
        "empty input should mention invoice_id, got: {err_msg}"
    );
}

/// assess_invoice_payment_risk with invalid UUID returns error.
#[tokio::test]
async fn test_assess_invoice_payment_risk_invalid_uuid_returns_error() {
    let ctx = agent_context_with_modules(vec![]);
    let registry = ToolRegistry::new(fake_pool());

    let result = registry
        .execute_tool("assess_invoice_payment_risk", &ctx, "not-a-uuid")
        .await;
    assert!(result.is_err(), "invalid UUID should return an error");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Invalid invoice ID format") || err_msg.contains("Invalid"),
        "invalid UUID should mention format, got: {err_msg}"
    );
}
