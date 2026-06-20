//! Unit-level coverage for the ContinuousLearningEngine logic that does not
//! need a database. Database-backed tenant-isolation cases are exercised
//! separately in the API integration suite.
//!
//! Issue #404.

use billforge_invoice_processing::continuous_learning::{
    monday_of, CorrectionType, CorrectionsByKind,
};
use chrono::NaiveDate;

#[test]
fn correction_type_round_trips_through_string() {
    for kind in [
        CorrectionType::GlRecode,
        CorrectionType::ApproverReroute,
        CorrectionType::AutopilotOverride,
        CorrectionType::DuplicateDismissal,
    ] {
        let s = kind.as_str();
        assert_eq!(CorrectionType::from_str(s), Some(kind));
    }
}

#[test]
fn unknown_correction_type_is_rejected() {
    assert!(CorrectionType::from_str("invalid").is_none());
}

#[test]
fn corrections_by_kind_totals_all_four_kinds() {
    let mut c = CorrectionsByKind::default();
    c.bump(CorrectionType::GlRecode);
    c.bump(CorrectionType::GlRecode);
    c.bump(CorrectionType::GlRecode);
    c.bump(CorrectionType::ApproverReroute);
    c.bump(CorrectionType::AutopilotOverride);
    c.bump(CorrectionType::DuplicateDismissal);
    c.bump(CorrectionType::DuplicateDismissal);

    assert_eq!(c.gl_recode, 3);
    assert_eq!(c.approver_reroute, 1);
    assert_eq!(c.autopilot_override, 1);
    assert_eq!(c.duplicate_dismissal, 2);
    assert_eq!(c.total(), 7);
}

#[test]
fn monday_of_anchors_week_start_correctly() {
    let monday = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
    let wednesday = NaiveDate::from_ymd_opt(2026, 6, 17).unwrap();
    let sunday = NaiveDate::from_ymd_opt(2026, 6, 21).unwrap();
    assert_eq!(monday_of(monday), monday);
    assert_eq!(monday_of(wednesday), monday);
    assert_eq!(monday_of(sunday), monday);
}

#[test]
fn correction_type_serializes_snake_case() {
    let val = serde_json::to_value(CorrectionType::GlRecode).unwrap();
    assert_eq!(val, serde_json::json!("gl_recode"));
    let val = serde_json::to_value(CorrectionType::ApproverReroute).unwrap();
    assert_eq!(val, serde_json::json!("approver_reroute"));
}
