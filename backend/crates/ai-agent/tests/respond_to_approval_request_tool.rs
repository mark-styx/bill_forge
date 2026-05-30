//! Tests for the respond_to_approval_request mutating AP-action tool.

use billforge_ai_agent::tools::{
    AiToolPermission, AiToolRiskLevel, ToolProposalContext, ToolRegistry,
};

// ── (a) Tool definition metadata ─────────────────────────────────────────────

#[test]
fn test_respond_to_approval_request_definition_is_registered() {
    let def = ToolRegistry::get_tool_definition("respond_to_approval_request")
        .expect("respond_to_approval_request should be registered");

    assert_eq!(def.name, "respond_to_approval_request");
    assert!(def.mutates, "must be a mutating tool");
    assert_eq!(def.risk_level, AiToolRiskLevel::High);
    assert_eq!(def.required_permission, AiToolPermission::ApprovalRespond);
}

// ── (b) Mutation guard fires without approved proposal context ────────────────

#[test]
fn test_respond_to_approval_request_requires_approved_proposal_context() {
    let def = ToolRegistry::get_tool_definition("respond_to_approval_request")
        .expect("respond_to_approval_request should be registered");

    // Without any proposal context, the guard must reject.
    let result = ToolRegistry::validate_tool_execution_guard(&def, None);
    let err = result.expect_err("mutating tool should require approved proposal context");
    assert!(
        err.to_string().contains("requires an approved proposal context"),
        "error message should mention approved proposal context, got: {}",
        err
    );

    // With a non-approved / mismatched proposal context, the guard must also reject.
    let bad_contexts = [
        ToolProposalContext {
            proposal_id: uuid::Uuid::new_v4(),
            tool_name: "respond_to_approval_request".to_string(),
            approved: false,
        },
        ToolProposalContext {
            proposal_id: uuid::Uuid::new_v4(),
            tool_name: "other_tool".to_string(),
            approved: true,
        },
    ];
    for ctx in &bad_contexts {
        let result = ToolRegistry::validate_tool_execution_guard(&def, Some(ctx));
        assert!(
            result.is_err(),
            "invalid proposal context should be rejected"
        );
    }

    // With a correct, approved proposal context, the guard passes.
    let good_ctx = ToolProposalContext {
        proposal_id: uuid::Uuid::new_v4(),
        tool_name: "respond_to_approval_request".to_string(),
        approved: true,
    };
    let result = ToolRegistry::validate_tool_execution_guard(&def, Some(&good_ctx));
    assert!(
        result.is_ok(),
        "approved matching proposal context should pass"
    );
}
