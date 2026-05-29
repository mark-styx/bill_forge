//! Regression test: payment-execution surface must not leak into the API.
//!
//! Per northstar, BillForge does not perform payment execution.  This test
//! locks that exclusion at the API boundary by checking:
//!   1. The v1 router has no `/payment-requests` route.
//!   2. `GoLiveChecks` serialization does not contain a `schedule_first_payment_run` key.

use billforge_api::openapi::openapi_doc;
use billforge_api::routes::implementation::GoLiveChecks;

/// Walk the serialized OpenAPI paths and assert no path contains `/payment-requests`.
/// If a route surface for payment execution is re-introduced (even behind a feature
/// flag that flips on in CI), this test fails and blocks the merge.
#[test]
fn openapi_paths_exclude_payment_requests() {
    let spec = openapi_doc();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let paths = parsed["paths"]
        .as_object()
        .expect("paths should be a JSON object");

    let offenders: Vec<&String> = paths
        .keys()
        .filter(|p| p.contains("/payment-requests"))
        .collect();

    assert!(
        offenders.is_empty(),
        "OpenAPI spec must not expose any /payment-requests paths (payment execution \
         is excluded from the northstar), but found: {offenders:?}"
    );
}

/// Verify that `GoLiveChecks` does not carry a `schedule_first_payment_run` field.
/// If someone re-adds the field this assertion catches it immediately.
#[test]
fn go_live_checks_excludes_payment_run_key() {
    let checks = GoLiveChecks {
        notify_ap_team: true,
        set_email_forwarding: true,
        enable_approval_routing: true,
        confirm_cutover_date: true,
    };

    let json = serde_json::to_string(&checks).expect("serialize GoLiveChecks");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("re-parse JSON");

    // The key must NOT be present at all.
    assert!(
        parsed.get("schedule_first_payment_run").is_none(),
        "GoLiveChecks must not contain schedule_first_payment_run, but found: {json}"
    );

    // Sanity: the expected keys are all present and true.
    assert_eq!(parsed["notify_ap_team"], true);
    assert_eq!(parsed["set_email_forwarding"], true);
    assert_eq!(parsed["enable_approval_routing"], true);
    assert_eq!(parsed["confirm_cutover_date"], true);
}
