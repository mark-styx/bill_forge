//! Tests for the forecast-tuning worker (issue #367).
//!
//! Mirrors the pattern in `routing_optimization_test.rs`: pure-logic tests of
//! the decision function that turns observed accuracy metrics into
//! `ArimaForecaster` parameter overrides. The DB-backed path
//! (`run_tenant_forecast_tuning`) is covered structurally by the tenant
//! scoping contract: every query is bound to `tenant_id` and runs against the
//! tenant-specific pool returned by `pg_manager.tenant()`.
//!
//! The decision rules under test (from the plan):
//! - No history -> no tuning row written (no panic).
//! - MAPE > 25% -> CI multiplier widened to 1.25.
//! - |signed bias| > 10% -> seasonality threshold loosened by 0.05.
//! - All overrides clamped to safe ranges.
//! - When history exists the observed MAPE is always recorded.

use billforge_core::types::TenantId;
use billforge_worker::jobs::forecast_tuning::compute_tuning_decision;
use uuid::Uuid;

/// Empty history must never write a tuning row, regardless of how bad the
/// aggregated metrics would be if they were meaningful.
#[test]
fn empty_history_writes_no_tuning() {
    assert_eq!(compute_tuning_decision(0.0, 0.0, 0), None);
    assert_eq!(compute_tuning_decision(99.0, 99.0, 0), None);
    // Defensive: negative sample counts are treated as empty too.
    assert_eq!(compute_tuning_decision(99.0, 99.0, -5), None);
}

/// High MAPE (above 25%) widens the CI multiplier to 1.25 and records the
/// observed MAPE, without touching the seasonality threshold or level.
#[test]
fn high_mape_widens_ci_multiplier() {
    let d = compute_tuning_decision(30.0, 0.0, 10).expect("history present");
    assert_eq!(d.ci_width_multiplier, Some(1.25));
    assert!(
        d.seasonality_threshold_override.is_none(),
        "MAPE alone must not loosen the seasonality threshold"
    );
    assert!(
        d.level_bias_correction.is_none(),
        "MAPE alone must not shift the projected level"
    );
    assert_eq!(d.mape_30d, Some(30.0));
}

/// MAPE exactly at the 25% threshold is not strictly greater, so no widening.
#[test]
fn mape_at_threshold_does_not_trigger_widening() {
    let d = compute_tuning_decision(25.0, 0.0, 10).expect("history present");
    assert!(d.ci_width_multiplier.is_none());
}

/// |signed bias| above 10% loosens the seasonality threshold by 0.05
/// (0.5 -> 0.45), regardless of sign.
#[test]
fn positive_bias_loosens_seasonality_threshold() {
    let d = compute_tuning_decision(10.0, 15.0, 10).expect("history present");
    assert_eq!(d.seasonality_threshold_override, Some(0.45));
    assert!(
        d.ci_width_multiplier.is_none(),
        "bias alone must not widen the CI"
    );
}

#[test]
fn negative_bias_also_loosens_seasonality_threshold() {
    // Undershooting forecasts are just as biased as overshooting ones.
    let d = compute_tuning_decision(10.0, -15.0, 10).expect("history present");
    assert_eq!(d.seasonality_threshold_override, Some(0.45));
}

/// |signed bias| exactly at 10% is not strictly greater, so no loosening.
#[test]
fn bias_at_threshold_does_not_trigger_loosening() {
    let d = compute_tuning_decision(5.0, 10.0, 10).expect("history present");
    assert!(d.seasonality_threshold_override.is_none());
    assert!(d.ci_width_multiplier.is_none());
    assert!(d.level_bias_correction.is_none());
}

/// Issue #398: positive (overshoot) bias above the threshold emits a negative
/// `level_bias_correction` so the next forecast's projected level shrinks.
/// 20% bias * 0.5 damping = 0.10, sign-flipped = -0.10.
#[test]
fn positive_bias_emits_negative_level_correction() {
    let d = compute_tuning_decision(10.0, 20.0, 30).expect("history present");
    let c = d.level_bias_correction.expect("expected a correction");
    assert!(
        (c - (-0.10)).abs() < 1e-9,
        "expected -0.10, got {}",
        c
    );
}

/// Issue #398: negative (undershoot) bias above the threshold emits a positive
/// `level_bias_correction` so the next forecast's projected level grows.
#[test]
fn negative_bias_emits_positive_level_correction() {
    let d = compute_tuning_decision(10.0, -20.0, 30).expect("history present");
    let c = d.level_bias_correction.expect("expected a correction");
    assert!(
        (c - 0.10).abs() < 1e-9,
        "expected 0.10, got {}",
        c
    );
}

/// Issue #398: |bias| at the 10% threshold (not strictly greater) must not
/// emit a level correction. Pairs with `bias_at_threshold_does_not_trigger_loosening`.
#[test]
fn bias_at_threshold_emits_no_level_correction() {
    let d = compute_tuning_decision(5.0, 10.0, 10).expect("history present");
    assert!(d.level_bias_correction.is_none());
}

/// When both MAPE and bias are high, both overrides are applied and the row
/// still records the observed MAPE.
#[test]
fn high_mape_and_high_bias_apply_both_overrides() {
    let d = compute_tuning_decision(40.0, 20.0, 25).expect("history present");
    assert_eq!(d.ci_width_multiplier, Some(1.25));
    assert_eq!(d.seasonality_threshold_override, Some(0.45));
    assert_eq!(d.mape_30d, Some(40.0));
}

/// Good accuracy and unbiased forecasts still produce a row (so `mape_30d` is
/// tracked for observability), but with `None` overrides so `ArimaForecaster`
/// falls back to its defaults.
#[test]
fn low_mape_low_bias_records_mape_only() {
    let d = compute_tuning_decision(5.0, 2.0, 50).expect("history present");
    assert!(d.ci_width_multiplier.is_none());
    assert!(d.seasonality_threshold_override.is_none());
    assert!(d.level_bias_correction.is_none());
    assert_eq!(d.mape_30d, Some(5.0));
}

/// Even with absurd inputs the computed overrides stay inside their declared
/// safe bands, so the worker can never persist a degenerate value.
#[test]
fn overrides_clamp_to_safe_ranges() {
    let d = compute_tuning_decision(1_000_000.0, 1_000_000.0, 100)
        .expect("history present");
    if let Some(ci) = d.ci_width_multiplier {
        assert!((1.0..=2.0).contains(&ci), "CI multiplier out of band: {}", ci);
    }
    if let Some(th) = d.seasonality_threshold_override {
        assert!(
            (0.1..=0.9).contains(&th),
            "seasonality threshold out of band: {}",
            th
        );
    }
    if let Some(c) = d.level_bias_correction {
        assert!(
            (-0.5..=0.5).contains(&c),
            "level_bias_correction out of band: {}",
            c
        );
    }
}

/// TenantId round-trip sanity: the worker resolves tenants through
/// `pg_manager.tenant(&TenantId)`, so a parsed TenantId must equal the
/// original. This is the same isolation contract `routing_optimization_test`
/// asserts for the routing loop.
#[test]
fn tenant_id_roundtrip_for_pool_lookup() {
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let parsed: TenantId = tenant_id
        .as_str()
        .parse()
        .expect("TenantId should round-trip through its string form");
    assert_eq!(tenant_id, parsed);
}
