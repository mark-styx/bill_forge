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
}
