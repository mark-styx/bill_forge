/// Tripwire test: ensures the PARKED notice in lib.rs cannot be silently removed.
///
/// If anyone strips the parked notice without going through the reactivation
/// gates documented in docs/edi_integration_plan.md, this test will fail in CI.

#[test]
fn parked_status_documented() {
    let lib_rs = include_str!("../src/lib.rs");

    assert!(
        lib_rs.contains("Status: PARKED"),
        "lib.rs must contain 'Status: PARKED'. If this was intentionally removed, \
         ensure the reactivation criteria in docs/edi_integration_plan.md are met first."
    );

    assert!(
        lib_rs.to_lowercase().contains("northstar"),
        "lib.rs must reference northstar to maintain governance traceability."
    );
}
