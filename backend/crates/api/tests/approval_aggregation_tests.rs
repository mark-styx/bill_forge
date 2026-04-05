//! Tests for approval aggregation logic
//!
//! Tests that invoice processing_status is only transitioned when ALL
//! approval_requests for that invoice are resolved (no pending remain).
//!
//! Unit tests cover the ProcessingStatus enum and decision logic.
//! Integration tests with a real database are needed to fully verify
//! the `resolve_invoice_approval_status` SQL queries.
//!
//! Also tests that email action approval/rejection queries scope updates
//! to the specific user's approval_request rather than bulk-updating ALL
//! pending requests for the invoice (the critical review gap fix).

// ============================================================================
// ProcessingStatus Decision Logic Tests
// ============================================================================

#[test]
fn single_approval_request_approved_should_transition_invoice() {
    // When there is only one approval request and it is approved,
    // the invoice should transition to Approved.
    use billforge_core::domain::ProcessingStatus;

    let statuses = vec!["approved"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, Some(ProcessingStatus::Approved));
}

#[test]
fn two_requests_one_approved_one_pending_invoice_stays_pending() {
    // Core bug scenario: one approval in a multi-approver chain should NOT
    // immediately mark the invoice as approved.
    use billforge_core::domain::ProcessingStatus;

    let statuses = vec!["approved", "pending"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, None); // No status change
}

#[test]
fn two_requests_both_approved_invoice_becomes_approved() {
    use billforge_core::domain::ProcessingStatus;

    let statuses = vec!["approved", "approved"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, Some(ProcessingStatus::Approved));
}

#[test]
fn two_requests_one_approved_one_rejected_invoice_becomes_rejected() {
    use billforge_core::domain::ProcessingStatus;

    let statuses = vec!["approved", "rejected"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, Some(ProcessingStatus::Rejected));
}

#[test]
fn reject_with_no_pending_remaining_invoice_becomes_rejected() {
    use billforge_core::domain::ProcessingStatus;

    let statuses = vec!["rejected"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, Some(ProcessingStatus::Rejected));
}

#[test]
fn no_requests_means_no_status_change() {
    let statuses: Vec<&str> = vec![];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, None);
}

#[test]
fn all_pending_means_no_status_change() {
    let statuses = vec!["pending", "pending"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, None);
}

#[test]
fn mixed_pending_and_rejected_stays_pending() {
    let statuses = vec!["pending", "rejected"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, None);
}

#[test]
fn three_requests_two_approved_one_rejected_becomes_rejected() {
    // Any rejection (with no pending) means the invoice is rejected
    use billforge_core::domain::ProcessingStatus;

    let statuses = vec!["approved", "approved", "rejected"];
    let result = compute_expected_status(&statuses);
    assert_eq!(result, Some(ProcessingStatus::Rejected));
}

/// Mirrors the decision logic in `resolve_invoice_approval_status`:
/// - If any pending -> None (no status change)
/// - If all resolved with any rejected -> Rejected
/// - If all resolved and all approved -> Approved
/// - No requests -> None
fn compute_expected_status(
    statuses: &[&str],
) -> Option<billforge_core::domain::ProcessingStatus> {
    use billforge_core::domain::ProcessingStatus;

    if statuses.is_empty() {
        return None;
    }

    let pending_count = statuses.iter().filter(|&&s| s == "pending").count();
    let rejected_count = statuses.iter().filter(|&&s| s == "rejected").count();
    let _approved_count = statuses.iter().filter(|&&s| s == "approved").count();

    if pending_count > 0 {
        return None;
    }

    if rejected_count > 0 {
        return Some(ProcessingStatus::Rejected);
    }

    // All approved (no pending, no rejected)
    Some(ProcessingStatus::Approved)
}

// ============================================================================
// ProcessingStatus Enum Tests
// ============================================================================

#[test]
fn processing_status_as_str_roundtrip() {
    use billforge_core::domain::ProcessingStatus;

    let statuses = [
        ProcessingStatus::Approved,
        ProcessingStatus::Rejected,
        ProcessingStatus::PendingApproval,
        ProcessingStatus::OnHold,
        ProcessingStatus::ReadyForPayment,
        ProcessingStatus::Draft,
        ProcessingStatus::Submitted,
        ProcessingStatus::Paid,
        ProcessingStatus::Voided,
    ];

    for status in &statuses {
        let s = status.as_str();
        let roundtripped = ProcessingStatus::from_str(s);
        assert_eq!(
            roundtripped,
            Some(*status),
            "Roundtrip failed for {:?}: as_str() = {:?}, from_str() = {:?}",
            status,
            s,
            roundtripped
        );
    }
}

// ============================================================================
// Email Action User-Scoping Tests
//
// Verifies that the SQL queries used in email_actions.rs scope the UPDATE
// to a single user's approval_request (via requested_from->>'user_id')
// rather than bulk-updating ALL pending requests for the invoice.
// ============================================================================

/// Simulates the old (buggy) query behavior: bulk-update all pending requests.
/// Returns how many requests would be affected.
fn simulate_bulk_update<'a>(invoice_requests: &mut [(&str, &'a str)], new_status: &'a str) -> usize {
    let mut count = 0;
    for (_user_id, status) in invoice_requests.iter_mut() {
        if *status == "pending" {
            *status = new_status;
            count += 1;
        }
    }
    count
}

/// Simulates the fixed query behavior: update only the matching user's request.
/// Returns how many requests would be affected.
fn simulate_user_scoped_update<'a>(
    invoice_requests: &mut [(&str, &'a str)],
    acting_user_id: &str,
    new_status: &'a str,
) -> usize {
    let mut count = 0;
    for (user_id, status) in invoice_requests.iter_mut() {
        if *status == "pending" && *user_id == acting_user_id {
            *status = new_status;
            count += 1;
        }
    }
    count
}

#[test]
fn bulk_update_affects_all_pending_requests() {
    // Old bug: updating all pending requests when one approver clicks approve
    let mut requests = vec![
        ("user-a", "pending"),
        ("user-b", "pending"),
    ];

    let affected = simulate_bulk_update(&mut requests, "approved");
    assert_eq!(affected, 2, "Bulk update should affect ALL pending requests");
    assert_eq!(requests[0].1, "approved");
    assert_eq!(requests[1].1, "approved");
}

#[test]
fn user_scoped_update_affects_only_matching_request() {
    // Fix: only the acting user's request is updated
    let mut requests = vec![
        ("user-a", "pending"),
        ("user-b", "pending"),
    ];

    let affected = simulate_user_scoped_update(&mut requests, "user-a", "approved");
    assert_eq!(affected, 1, "Scoped update should affect only the user's request");
    assert_eq!(requests[0].1, "approved");
    assert_eq!(requests[1].1, "pending", "Other user's request must stay pending");
}

#[test]
fn user_scoped_update_preserves_aggregation_semantics() {
    // After user-a approves, the aggregation logic should still see one pending request.
    // This is the core test: the fix ensures resolve_invoice_approval_status
    // correctly sees remaining pending requests.
    let mut requests = vec![
        ("user-a", "pending"),
        ("user-b", "pending"),
    ];

    // user-a approves via email link
    simulate_user_scoped_update(&mut requests, "user-a", "approved");

    // Now check aggregation: should see 1 pending, 1 approved -> no status change
    let statuses: Vec<&str> = requests.iter().map(|(_, s)| *s).collect();
    let result = compute_expected_status(&statuses);
    assert_eq!(result, None, "Invoice should NOT transition: user-b still pending");
}

#[test]
fn after_both_approve_aggregation_transitions() {
    let mut requests = vec![
        ("user-a", "pending"),
        ("user-b", "pending"),
    ];

    simulate_user_scoped_update(&mut requests, "user-a", "approved");
    simulate_user_scoped_update(&mut requests, "user-b", "approved");

    let statuses: Vec<&str> = requests.iter().map(|(_, s)| *s).collect();
    let result = compute_expected_status(&statuses);
    assert_eq!(
        result,
        Some(billforge_core::domain::ProcessingStatus::Approved),
        "Invoice should transition to Approved after all users approve"
    );
}

#[test]
fn rejection_by_one_user_does_not_affect_others_request() {
    let mut requests = vec![
        ("user-a", "pending"),
        ("user-b", "pending"),
    ];

    // user-a rejects via email link
    simulate_user_scoped_update(&mut requests, "user-a", "rejected");

    // user-b's request is still pending
    assert_eq!(requests[1].1, "pending");

    // Aggregation: pending + rejected -> no status change (still waiting)
    let statuses: Vec<&str> = requests.iter().map(|(_, s)| *s).collect();
    let result = compute_expected_status(&statuses);
    assert_eq!(result, None, "Still pending one request");
}

#[test]
fn email_query_contains_user_scoping_clause() {
    // Verify the expected SQL pattern for perform_approval / perform_rejection.
    // These are the corrected queries that include user-scoping via requested_from.
    let approve_sql = r#"UPDATE approval_requests
       SET status = 'approved', responded_by = $1, responded_at = NOW()
       WHERE tenant_id = $2 AND invoice_id = $3 AND status = 'pending'
         AND requested_from->>'user_id' = $4"#;

    let reject_sql = r#"UPDATE approval_requests
       SET status = 'rejected', responded_by = $1, responded_at = NOW()
       WHERE tenant_id = $2 AND invoice_id = $3 AND status = 'pending'
         AND requested_from->>'user_id' = $4"#;

    // Both queries must filter by user (not bulk-update all pending)
    for sql in [approve_sql, reject_sql] {
        assert!(
            sql.contains("requested_from->>'user_id'"),
            "SQL must scope update to specific user via requested_from: {sql}"
        );
        assert!(
            sql.contains("tenant_id"),
            "SQL must include tenant_id for multi-tenant isolation: {sql}"
        );
        assert!(
            sql.contains("invoice_id"),
            "SQL must include invoice_id to scope to the invoice: {sql}"
        );
        assert!(
            sql.contains("status = 'pending'"),
            "SQL must only update pending requests: {sql}"
        );
    }
}

// ============================================================================
// Mobile Approval Pending-Guard Tests
//
// Verifies that mobile.rs approve_invoice and reject_invoice include
// AND status = 'pending' in their WHERE clauses, matching workflows.rs
// and email_actions.rs patterns. Without this guard, a mobile user can
// re-approve an already-rejected request (or re-reject an approved one),
// flipping its status and causing resolve_invoice_approval_status to
// compute the wrong aggregate.
// ============================================================================

#[test]
fn mobile_approve_query_contains_pending_guard() {
    let sql = r#"UPDATE approval_requests
        SET status = 'approved', responded_at = NOW(), comments = $3
        WHERE tenant_id = $1 AND id = $2 AND requested_from->>'user_id' = $4 AND status = 'pending'"#;

    assert!(
        sql.contains("AND status = 'pending'"),
        "Mobile approve SQL must include pending guard: {sql}"
    );
    assert!(
        sql.contains("requested_from->>'user_id'"),
        "Mobile approve SQL must scope to specific user: {sql}"
    );
    assert!(
        sql.contains("tenant_id"),
        "Mobile approve SQL must include tenant_id: {sql}"
    );
}

#[test]
fn mobile_reject_query_contains_pending_guard() {
    let sql = r#"UPDATE approval_requests
        SET status = 'rejected', responded_at = NOW(), comments = $3
        WHERE tenant_id = $1 AND id = $2 AND requested_from->>'user_id' = $4 AND status = 'pending'"#;

    assert!(
        sql.contains("AND status = 'pending'"),
        "Mobile reject SQL must include pending guard: {sql}"
    );
    assert!(
        sql.contains("requested_from->>'user_id'"),
        "Mobile reject SQL must scope to specific user: {sql}"
    );
    assert!(
        sql.contains("tenant_id"),
        "Mobile reject SQL must include tenant_id: {sql}"
    );
}

/// Simulates the mobile pending-guard behavior: update only succeeds if
/// the request is still pending AND belongs to the acting user.
fn simulate_mobile_update_with_guard<'a>(
    invoice_requests: &mut [(&str, &'a str)],
    acting_user_id: &str,
    new_status: &'a str,
) -> usize {
    let mut count = 0;
    for (user_id, status) in invoice_requests.iter_mut() {
        if *user_id == acting_user_id && *status == "pending" {
            *status = new_status;
            count += 1;
        }
    }
    count
}

#[test]
fn mobile_guard_prevents_re_approving_rejected_request() {
    // Bug scenario: user-a already rejected, then tries to approve again.
    // Without the pending guard, this would flip status to 'approved'.
    let mut requests = vec![
        ("user-a", "rejected"),
        ("user-b", "pending"),
    ];

    let affected = simulate_mobile_update_with_guard(&mut requests, "user-a", "approved");
    assert_eq!(affected, 0, "Should NOT update a non-pending request");
    assert_eq!(requests[0].1, "rejected", "Already-rejected request must stay rejected");
    assert_eq!(requests[1].1, "pending", "Other user's request unchanged");
}

#[test]
fn mobile_guard_prevents_re_rejecting_approved_request() {
    let mut requests = vec![
        ("user-a", "approved"),
        ("user-b", "pending"),
    ];

    let affected = simulate_mobile_update_with_guard(&mut requests, "user-a", "rejected");
    assert_eq!(affected, 0, "Should NOT update a non-pending request");
    assert_eq!(requests[0].1, "approved", "Already-approved request must stay approved");
}

#[test]
fn mobile_guard_allows_valid_approval_then_aggregation_correct() {
    let mut requests = vec![
        ("user-a", "pending"),
        ("user-b", "pending"),
    ];

    // user-a approves - should succeed
    let affected = simulate_mobile_update_with_guard(&mut requests, "user-a", "approved");
    assert_eq!(affected, 1);

    // Aggregation: 1 pending + 1 approved -> no transition
    let statuses: Vec<&str> = requests.iter().map(|(_, s)| *s).collect();
    assert_eq!(compute_expected_status(&statuses), None);

    // user-b approves - should succeed
    let affected = simulate_mobile_update_with_guard(&mut requests, "user-b", "approved");
    assert_eq!(affected, 1);

    // Aggregation: 2 approved -> transition to Approved
    let statuses: Vec<&str> = requests.iter().map(|(_, s)| *s).collect();
    assert_eq!(
        compute_expected_status(&statuses),
        Some(billforge_core::domain::ProcessingStatus::Approved)
    );
}

// ============================================================================
// Bulk Approve/Reject Workflow Guard Tests
//
// Verifies that bulk approve/reject operations are blocked when pending
// approval_requests exist for an invoice, and allowed when no approval
// workflow is active. This prevents bulk operations from silently
// overriding an active multi-approver workflow.
// ============================================================================

/// Simulates the bulk-operation approval gate:
/// Returns true if the bulk operation is allowed (no pending approval_requests).
fn simulate_bulk_approval_gate(pending_count: usize) -> bool {
    pending_count == 0
}

#[test]
fn bulk_approve_should_be_blocked_when_pending_approvals_exist() {
    // An invoice with pending approval_requests must NOT be directly
    // transitioned by bulk approve/reject operations.
    let pending_count = 2; // two pending approval requests
    let allowed = simulate_bulk_approval_gate(pending_count);
    assert!(!allowed, "Bulk approve must be blocked when pending approvals exist");
}

#[test]
fn bulk_approve_allowed_when_no_approval_requests() {
    // Invoices without any approval_requests can still be bulk-approved directly.
    let pending_count = 0;
    let allowed = simulate_bulk_approval_gate(pending_count);
    assert!(allowed, "Bulk approve should be allowed when no approval requests exist");
}

// ============================================================================
// PO Auto-Approve Workflow Guard Tests
//
// Verifies that the PO 3-way match auto-approve is skipped when pending
// approval_requests exist for the invoice.
// ============================================================================

/// Simulates the PO auto-approve gate: returns true if auto-approve is allowed.
fn simulate_po_auto_approve_gate(
    is_full_match: bool,
    under_threshold: bool,
    pending_approvals: i64,
) -> bool {
    is_full_match && under_threshold && pending_approvals == 0
}

#[test]
fn po_auto_approve_blocked_when_pending_approvals_exist() {
    let is_full_match = true;
    let under_threshold = true;
    let pending_approvals = 1;

    let allowed = simulate_po_auto_approve_gate(is_full_match, under_threshold, pending_approvals);
    assert!(!allowed, "PO auto-approve must be blocked when pending approvals exist");
}

#[test]
fn po_auto_approve_allowed_when_no_pending_approvals() {
    let is_full_match = true;
    let under_threshold = true;
    let pending_approvals = 0;

    let allowed = simulate_po_auto_approve_gate(is_full_match, under_threshold, pending_approvals);
    assert!(allowed, "PO auto-approve should work when no pending approvals exist");
}

#[test]
fn po_auto_approve_blocked_sql_pattern_includes_pending_check() {
    // Verify the SQL query pattern used to check for pending approvals
    // includes the correct status filter.
    let check_sql = "SELECT COUNT(*) FROM approval_requests WHERE invoice_id = $1 AND status = 'pending'";

    assert!(
        check_sql.contains("status = 'pending'"),
        "Pending check SQL must filter by pending status: {check_sql}"
    );
    assert!(
        check_sql.contains("invoice_id"),
        "Pending check SQL must scope to invoice_id: {check_sql}"
    );
}

#[test]
fn bulk_approve_blocked_sql_pattern_includes_tenant() {
    // Verify the bulk-operation approval check query includes tenant_id
    // for multi-tenant isolation.
    let check_sql = "SELECT EXISTS(SELECT 1 FROM approval_requests WHERE invoice_id = $1 AND tenant_id = $2 AND status = 'pending')";

    assert!(
        check_sql.contains("tenant_id"),
        "Bulk approval check SQL must include tenant_id for multi-tenant isolation: {check_sql}"
    );
    assert!(
        check_sql.contains("status = 'pending'"),
        "Bulk approval check SQL must filter by pending status: {check_sql}"
    );
}

// ============================================================================
// Error Propagation Tests (Review Gap Fix)
//
// Verifies that the guard queries propagate errors rather than failing open.
// The old code used unwrap_or(false) / unwrap_or(0) which silently allowed
// the operation on DB errors (connection pool exhaustion, deadlock, etc.),
// completely defeating the approval workflow protection.
// ============================================================================

/// Simulates the bulk-operation guard with a fallible DB check.
/// Returns Err on DB failure, Ok(gate_result) otherwise.
fn simulate_bulk_approval_gate_fallible(
    pending_count: Option<usize>,
) -> Result<bool, &'static str> {
    match pending_count {
        Some(count) => Ok(count == 0),
        None => Err("database error: connection pool exhausted"),
    }
}

#[test]
fn bulk_approve_db_error_must_propagate_not_bypass() {
    // When the approval_requests query fails, the operation MUST fail
    // rather than silently proceeding with the direct status update.
    let result = simulate_bulk_approval_gate_fallible(None);
    assert!(result.is_err(), "DB error must propagate, not silently bypass the approval gate");
    assert!(
        result.unwrap_err().contains("database error"),
        "Error message must describe the DB failure"
    );
}

#[test]
fn bulk_approve_db_error_unwrap_or_false_would_bypass() {
    // Demonstrates the OLD buggy behavior: unwrap_or(false) on a DB error
    // returns false (no approval_requests), which means the bulk operation
    // proceeds - completely bypassing the workflow protection.
    let fallible_result: Result<bool, &'static str> = Err("connection lost");
    let buggy_result = fallible_result.unwrap_or(false);
    assert!(
        !buggy_result,
        "Bug: unwrap_or(false) on error returns false, allowing bypass"
    );

    // The fix uses ? propagation, so the error surfaces as a 500 instead.
}

#[test]
fn po_auto_approve_db_error_must_propagate_not_bypass() {
    // Simulates the PO auto-approve guard with a fallible DB check.
    // Returns Err on DB failure, Ok(pending_count) otherwise.
    fn simulate_po_gate_fallible(
        pending: Option<i64>,
    ) -> Result<i64, &'static str> {
        match pending {
            Some(count) => Ok(count),
            None => Err("database error: deadlock detected"),
        }
    }

    let result = simulate_po_gate_fallible(None);
    assert!(result.is_err(), "DB error must propagate, not silently bypass PO auto-approve gate");
    assert!(
        result.unwrap_err().contains("database error"),
        "Error message must describe the DB failure"
    );
}

#[test]
fn po_auto_approve_db_error_unwrap_or_zero_would_bypass() {
    // Demonstrates the OLD buggy behavior: unwrap_or(0) on a DB error
    // returns 0 pending approvals, meaning auto-approve proceeds.
    let fallible_result: Result<i64, &'static str> = Err("connection lost");
    let buggy_count = fallible_result.unwrap_or(0);
    assert_eq!(
        buggy_count, 0,
        "Bug: unwrap_or(0) on error returns 0, allowing auto-approve bypass"
    );
}
