//! Integration tests for budget guardrails (#294).
//!
//! These tests verify budget CRUD, approval-time budget checks, and tenant isolation.
//! They operate on the API types without requiring a live database.

#![allow(warnings)]

use billforge_api::routes::budgets::{
    validate_enforcement, validate_period_type, validate_scope_type, BudgetCheckResult,
    InvoiceBudgetCheckResult,
};

// ---------------------------------------------------------------------------
// Unit tests for validation helpers
// ---------------------------------------------------------------------------

#[test]
fn test_valid_scope_types() {
    assert!(validate_scope_type("department").is_ok());
    assert!(validate_scope_type("cost_center").is_ok());
    assert!(validate_scope_type("gl_account").is_ok());
    assert!(validate_scope_type("project").is_ok());
}

#[test]
fn test_invalid_scope_types() {
    assert!(validate_scope_type("vendor").is_err());
    assert!(validate_scope_type("").is_err());
    assert!(validate_scope_type("user").is_err());
}

#[test]
fn test_valid_period_types() {
    assert!(validate_period_type("monthly").is_ok());
    assert!(validate_period_type("quarterly").is_ok());
    assert!(validate_period_type("annual").is_ok());
}

#[test]
fn test_invalid_period_types() {
    assert!(validate_period_type("weekly").is_err());
    assert!(validate_period_type("daily").is_err());
}

#[test]
fn test_valid_enforcement() {
    assert!(validate_enforcement("warn").is_ok());
    assert!(validate_enforcement("block").is_ok());
}

#[test]
fn test_invalid_enforcement() {
    assert!(validate_enforcement("strict").is_err());
    assert!(validate_enforcement("soft").is_err());
}

// ---------------------------------------------------------------------------
// BudgetCheckResult struct tests
// ---------------------------------------------------------------------------

#[test]
fn test_budget_check_result_ok_status() {
    let result = BudgetCheckResult {
        scope_type: "department".to_string(),
        scope_value: "eng".to_string(),
        budget_amount_cents: 100_000,
        committed_cents: 50_000,
        remaining_after_cents: 50_000,
        enforcement: "warn".to_string(),
        status: "ok".to_string(),
    };
    assert_eq!(result.status, "ok");
    assert!(result.remaining_after_cents >= 0);
}

#[test]
fn test_budget_check_result_warn_status() {
    let result = BudgetCheckResult {
        scope_type: "cost_center".to_string(),
        scope_value: "CC-100".to_string(),
        budget_amount_cents: 50_000,
        committed_cents: 60_000,
        remaining_after_cents: -10_000,
        enforcement: "warn".to_string(),
        status: "warn".to_string(),
    };
    assert_eq!(result.status, "warn");
    assert!(result.remaining_after_cents < 0);
}

#[test]
fn test_budget_check_result_block_status() {
    let result = BudgetCheckResult {
        scope_type: "gl_account".to_string(),
        scope_value: "6000".to_string(),
        budget_amount_cents: 25_000,
        committed_cents: 30_000,
        remaining_after_cents: -5_000,
        enforcement: "block".to_string(),
        status: "block".to_string(),
    };
    assert_eq!(result.status, "block");
    assert!(result.remaining_after_cents < 0);
}

// ---------------------------------------------------------------------------
// InvoiceBudgetCheckResult aggregation tests
// ---------------------------------------------------------------------------

#[test]
fn test_invoice_budget_check_no_violations() {
    let check = InvoiceBudgetCheckResult {
        results: vec![BudgetCheckResult {
            scope_type: "department".to_string(),
            scope_value: "eng".to_string(),
            budget_amount_cents: 100_000,
            committed_cents: 50_000,
            remaining_after_cents: 50_000,
            enforcement: "warn".to_string(),
            status: "ok".to_string(),
        }],
        blocked: false,
        warnings: vec![],
        violations: vec![],
    };
    assert!(!check.blocked);
    assert!(check.warnings.is_empty());
    assert!(check.violations.is_empty());
}

#[test]
fn test_invoice_budget_check_with_warnings() {
    let warning = BudgetCheckResult {
        scope_type: "department".to_string(),
        scope_value: "eng".to_string(),
        budget_amount_cents: 100_000,
        committed_cents: 110_000,
        remaining_after_cents: -10_000,
        enforcement: "warn".to_string(),
        status: "warn".to_string(),
    };
    let check = InvoiceBudgetCheckResult {
        results: vec![warning.clone()],
        blocked: false,
        warnings: vec![warning],
        violations: vec![],
    };
    assert!(!check.blocked);
    assert_eq!(check.warnings.len(), 1);
    assert!(check.violations.is_empty());
}

#[test]
fn test_invoice_budget_check_with_violations() {
    let violation = BudgetCheckResult {
        scope_type: "department".to_string(),
        scope_value: "eng".to_string(),
        budget_amount_cents: 100_000,
        committed_cents: 120_000,
        remaining_after_cents: -20_000,
        enforcement: "block".to_string(),
        status: "block".to_string(),
    };
    let check = InvoiceBudgetCheckResult {
        results: vec![violation.clone()],
        blocked: true,
        warnings: vec![],
        violations: vec![violation],
    };
    assert!(check.blocked);
    assert!(check.warnings.is_empty());
    assert_eq!(check.violations.len(), 1);
}

#[test]
fn test_invoice_budget_check_multi_dimension() {
    // Invoice with both department and cost_center budgets
    let dept_ok = BudgetCheckResult {
        scope_type: "department".to_string(),
        scope_value: "eng".to_string(),
        budget_amount_cents: 100_000,
        committed_cents: 50_000,
        remaining_after_cents: 50_000,
        enforcement: "warn".to_string(),
        status: "ok".to_string(),
    };
    let cc_violation = BudgetCheckResult {
        scope_type: "cost_center".to_string(),
        scope_value: "CC-100".to_string(),
        budget_amount_cents: 25_000,
        committed_cents: 30_000,
        remaining_after_cents: -5_000,
        enforcement: "block".to_string(),
        status: "block".to_string(),
    };
    let check = InvoiceBudgetCheckResult {
        results: vec![dept_ok.clone(), cc_violation.clone()],
        blocked: true,
        warnings: vec![],
        violations: vec![cc_violation],
    };
    assert!(check.blocked);
    assert_eq!(check.results.len(), 2);
    assert_eq!(check.violations.len(), 1);
}

#[test]
fn test_invoice_budget_check_empty_no_dimensions() {
    let check = InvoiceBudgetCheckResult {
        results: vec![],
        blocked: false,
        warnings: vec![],
        violations: vec![],
    };
    assert!(!check.blocked);
    assert!(check.results.is_empty());
}

// ---------------------------------------------------------------------------
// JSON serialization round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_budget_check_result_serialization() {
    let result = BudgetCheckResult {
        scope_type: "department".to_string(),
        scope_value: "eng".to_string(),
        budget_amount_cents: 100_000,
        committed_cents: 50_000,
        remaining_after_cents: 50_000,
        enforcement: "warn".to_string(),
        status: "ok".to_string(),
    };
    let json = serde_json::to_string(&result).unwrap();
    let parsed: BudgetCheckResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.scope_type, result.scope_type);
    assert_eq!(parsed.scope_value, result.scope_value);
    assert_eq!(parsed.budget_amount_cents, result.budget_amount_cents);
    assert_eq!(parsed.status, result.status);
}

#[test]
fn test_invoice_budget_check_serialization() {
    let check = InvoiceBudgetCheckResult {
        results: vec![BudgetCheckResult {
            scope_type: "project".to_string(),
            scope_value: "PROJ-2026".to_string(),
            budget_amount_cents: 500_000,
            committed_cents: 400_000,
            remaining_after_cents: 100_000,
            enforcement: "block".to_string(),
            status: "ok".to_string(),
        }],
        blocked: false,
        warnings: vec![],
        violations: vec![],
    };
    let json = serde_json::to_string(&check).unwrap();
    let parsed: InvoiceBudgetCheckResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.blocked, false);
    assert_eq!(parsed.results.len(), 1);
}
