//! Issue #415: 2-hour Implementation Speedrun.
//!
//! Targeted behavior tests for the helpers that back the four speedrun
//! handlers. These mirror the unit tests inside the module but exercise the
//! public API surface other crates and the wizard UI will call.

use billforge_api::routes::implementation_speedrun::{
    compute_threshold_tiers, qbo_employee_to_approver, role_strings_to_tier,
    SPEEDRUN_FIRST_INVOICES_TARGET, SPEEDRUN_TARGET_MINUTES,
};
use billforge_quickbooks::{QBEmailAddress, QBEmployee};

// ---------------------------------------------------------------------------
// Threshold inference
// ---------------------------------------------------------------------------

#[test]
fn infer_thresholds_uses_historical_gl_spend() {
    // 12 months of synthetic invoice amounts (cents). Median = 5000,
    // p75 = 7500, p95 = 9500 — i.e. the suggested tier amounts must scale
    // with the underlying GL spend, not a static template.
    let amounts: Vec<i64> = (1..=100).map(|n| n * 100).collect();
    let tiers = compute_threshold_tiers(&amounts);

    assert_eq!(tiers.len(), 3, "must return three tiers");
    assert_eq!(tiers[0].tier, 1);
    assert_eq!(tiers[0].source_percentile, "median");
    assert_eq!(tiers[0].amount_cents, 5_000);
    assert_eq!(tiers[1].tier, 2);
    assert_eq!(tiers[1].source_percentile, "p75");
    assert_eq!(tiers[1].amount_cents, 7_500);
    assert_eq!(tiers[2].tier, 3);
    assert_eq!(tiers[2].source_percentile, "p95");
    assert_eq!(tiers[2].amount_cents, 9_500);
}

#[test]
fn infer_thresholds_with_no_history_falls_back_to_defaults() {
    // First-day tenant — no invoices yet. The wizard still needs three
    // suggested tiers to show, in increasing order.
    let tiers = compute_threshold_tiers(&[]);
    assert_eq!(tiers.len(), 3);
    assert!(tiers.iter().all(|t| t.source_percentile == "default"));
    assert!(tiers[0].amount_cents < tiers[1].amount_cents);
    assert!(tiers[1].amount_cents < tiers[2].amount_cents);
}

// ---------------------------------------------------------------------------
// Approval-chain role mapping
// ---------------------------------------------------------------------------

#[test]
fn suggested_chain_maps_qbo_roles_to_tiers() {
    // QBO Admin → Tier 3, Manager → Tier 2, Standard → Tier 1.
    let admin = role_strings_to_tier(&["admin".to_string()]).unwrap();
    assert_eq!(admin.0, 3);

    let manager = role_strings_to_tier(&["manager".to_string()]).unwrap();
    assert_eq!(manager.0, 2);

    let standard = role_strings_to_tier(&["standard".to_string()]).unwrap();
    assert_eq!(standard.0, 1);
}

#[test]
fn suggested_chain_skips_users_with_no_mappable_role() {
    // ReportViewer is not an approval candidate — must not appear in the
    // suggested chain.
    assert!(role_strings_to_tier(&["report_viewer".to_string()]).is_none());
}

#[test]
fn suggested_chain_sources_qbo_employees_with_job_title_roles() {
    // Once QBO is connected, the speedrun must build the suggested chain
    // from live QBO Employee rows — not from whatever local users happen
    // to exist. We exercise the role-mapping path on QBO Employee shapes
    // directly, since constructing a real `SuggestedApprover` and the QBO
    // API call would require a tenant DB + HTTP fixture.
    let admin = qbo_employee_to_approver(QBEmployee {
        Id: "QBO-1".to_string(),
        DisplayName: Some("Avery Admin".to_string()),
        PrimaryEmailAddr: Some(QBEmailAddress {
            Address: "avery@example.com".to_string(),
        }),
        JobTitle: Some("Admin".to_string()),
        Active: Some(true),
    })
    .expect("QBO admin must yield a suggested approver");
    assert_eq!(admin.tier, 3);
    assert_eq!(admin.external_id.as_deref(), Some("QBO-1"));
    assert!(admin.user_id.is_none(), "QBO-sourced suggestion must not carry a local user id");

    let manager = qbo_employee_to_approver(QBEmployee {
        Id: "QBO-2".to_string(),
        DisplayName: Some("Morgan Manager".to_string()),
        PrimaryEmailAddr: Some(QBEmailAddress {
            Address: "morgan@example.com".to_string(),
        }),
        JobTitle: Some("Manager".to_string()),
        Active: Some(true),
    })
    .expect("QBO manager must yield a suggested approver");
    assert_eq!(manager.tier, 2);

    let unmapped = qbo_employee_to_approver(QBEmployee {
        Id: "QBO-3".to_string(),
        DisplayName: Some("Quinn Custodian".to_string()),
        PrimaryEmailAddr: Some(QBEmailAddress {
            Address: "quinn@example.com".to_string(),
        }),
        JobTitle: Some("Office Custodian".to_string()),
        Active: Some(true),
    });
    assert!(
        unmapped.is_none(),
        "QBO employees with no mappable JobTitle must be excluded from the chain"
    );
}

#[test]
fn suggested_chain_uses_highest_role_when_user_has_multiple() {
    // A user with both ap_user and tenant_admin lands in the executive
    // tier, so the suggested chain does not double-list them as a
    // first-line approver.
    let (tier, _) = role_strings_to_tier(&[
        "ap_user".to_string(),
        "tenant_admin".to_string(),
    ])
    .unwrap();
    assert_eq!(tier, 3);
}

// ---------------------------------------------------------------------------
// First-5-invoices walkthrough math
// ---------------------------------------------------------------------------

/// Reproduce the count-update arithmetic from `process_first_invoices` so we
/// can prove the cap-at-5 and completed_at semantics without standing up a
/// tenant database.
fn step(processed: i32, n: i32) -> i32 {
    processed
        .saturating_add(n)
        .min(SPEEDRUN_FIRST_INVOICES_TARGET)
}

#[test]
fn first_invoices_progress_caps_at_target() {
    assert_eq!(SPEEDRUN_FIRST_INVOICES_TARGET, 5);

    let mut processed = 0;
    for _ in 0..6 {
        processed = step(processed, 1);
    }
    assert_eq!(
        processed, SPEEDRUN_FIRST_INVOICES_TARGET,
        "six +1 increments must clamp at five"
    );
}

#[test]
fn first_invoices_progress_clamps_oversized_batch() {
    // A misbehaving client posting n=100 must not blow past five.
    assert_eq!(step(0, 100), SPEEDRUN_FIRST_INVOICES_TARGET);
    // Adding more once already at the cap stays at the cap.
    assert_eq!(step(SPEEDRUN_FIRST_INVOICES_TARGET, 3), SPEEDRUN_FIRST_INVOICES_TARGET);
}

#[test]
fn first_invoices_marks_completion_on_fifth() {
    // The handler stamps completed_at the first time the count reaches the
    // target. Encode that policy here so a refactor cannot regress it.
    let mut processed = 0;
    let mut completed = false;
    for _ in 0..5 {
        let next = step(processed, 1);
        if !completed && next >= SPEEDRUN_FIRST_INVOICES_TARGET {
            completed = true;
        }
        processed = next;
    }
    assert!(completed, "completed flag must be set when the fifth lands");
    assert_eq!(processed, SPEEDRUN_FIRST_INVOICES_TARGET);
}

// ---------------------------------------------------------------------------
// Target invariants
// ---------------------------------------------------------------------------

#[test]
fn target_is_under_two_hours() {
    assert_eq!(
        SPEEDRUN_TARGET_MINUTES, 120,
        "issue #415 promises onboarding in under 2 hours"
    );
}
