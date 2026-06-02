//! Unit tests for the OCR exception resolution workflow.
//!
//! The `resolve_ocr_exception` handler accepts `approve` or `reject` actions
//! and transitions `ocr_exception_status` from `pending` to the chosen state.
//! These tests exercise the validation predicate inline without a running database.

/// Mirrors the validation in `resolve_ocr_exception`:
///
/// ```ignore
/// let action = body.action.to_lowercase();
/// if action != "approve" && action != "reject" {
///     return Err(...)
/// }
/// ```
fn is_valid_resolve_action(action: &str) -> bool {
    matches!(action.to_lowercase().as_str(), "approve" | "reject")
}

#[test]
fn resolve_accepts_approve() {
    assert!(
        is_valid_resolve_action("approve"),
        "'approve' should be a valid resolve action"
    );
}

#[test]
fn resolve_accepts_reject() {
    assert!(
        is_valid_resolve_action("reject"),
        "'reject' should be a valid resolve action"
    );
}

#[test]
fn resolve_rejects_invalid_action() {
    assert!(
        !is_valid_resolve_action("foobar"),
        "'foobar' should not be a valid resolve action"
    );
}

#[test]
fn resolve_rejects_empty_string() {
    assert!(
        !is_valid_resolve_action(""),
        "empty string should not be a valid resolve action"
    );
}

#[test]
fn resolve_is_case_insensitive() {
    assert!(is_valid_resolve_action("Approve"));
    assert!(is_valid_resolve_action("REJECT"));
    assert!(is_valid_resolve_action("ApPrOvE"));
}

/// Simulates the status transition: pending -> approved/rejected.
fn next_ocr_exception_status(current: &str, action: &str) -> Result<String, &'static str> {
    if current != "pending" {
        return Err("already resolved");
    }
    match action.to_lowercase().as_str() {
        "approve" => Ok("approved".to_string()),
        "reject" => Ok("rejected".to_string()),
        _ => Err("invalid action"),
    }
}

#[test]
fn pending_approve_transitions_to_approved() {
    let result = next_ocr_exception_status("pending", "approve").unwrap();
    assert_eq!(result, "approved");
}

#[test]
fn pending_reject_transitions_to_rejected() {
    let result = next_ocr_exception_status("pending", "reject").unwrap();
    assert_eq!(result, "rejected");
}

#[test]
fn already_approved_cannot_transition() {
    let result = next_ocr_exception_status("approved", "reject");
    assert!(result.is_err());
}

#[test]
fn already_rejected_cannot_transition() {
    let result = next_ocr_exception_status("rejected", "approve");
    assert!(result.is_err());
}

#[test]
fn invalid_action_returns_error() {
    let result = next_ocr_exception_status("pending", "delete");
    assert!(result.is_err());
}
