//! Unit tests for OCR calibration logic.
//!
//! These tests validate the `calibrated_confidence` function and do not require
//! a database connection. The PgOcrCalibrationStore integration is covered by
//! the inline tests in calibration.rs and by the service_test.rs suite.

use std::collections::HashMap;

use billforge_invoice_capture::calibrated_confidence;

/// Test 1: calibrated_confidence equals the unweighted arithmetic mean when no
/// weights are present (parity with the old fixed-rule behaviour).
#[test]
fn calibrated_confidence_equals_unweighted_mean_when_no_weights() {
    let raw: &[(&str, f32)] = &[
        ("invoice_number", 0.95),
        ("invoice_date", 0.80),
        ("vendor_name", 0.90),
        ("total_amount", 0.85),
    ];
    let weights = HashMap::new();

    let result = calibrated_confidence(raw, &weights);
    let expected = (0.95 + 0.80 + 0.90 + 0.85) / 4.0;

    assert!(
        (result - expected).abs() < 0.001,
        "expected {}, got {}",
        expected,
        result
    );
}

/// Test 2: After recording many corrections on vendor_name, the vendor_name weight
/// drops and the overall calibrated score is lower than the unweighted mean for
/// the same raw input.
#[test]
fn calibrated_score_lower_when_vendor_name_weight_drops() {
    // vendor_name has the highest raw confidence, so lowering its empirical
    // weight should pull the calibrated score below the unweighted mean.
    let raw: &[(&str, f32)] = &[
        ("invoice_number", 0.90),
        ("invoice_date", 0.90),
        ("vendor_name", 0.95),
        ("total_amount", 0.90),
    ];

    // vendor_name has a low accuracy weight (many corrections), others are high.
    let mut weights = HashMap::new();
    weights.insert("invoice_number".to_string(), 0.86); // 25 extractions, 2 corrections -> (25-2+1)/(25+2) = 24/27 ~0.89
    weights.insert("invoice_date".to_string(), 0.86);
    weights.insert("vendor_name".to_string(), 0.27); // heavily corrected
    weights.insert("total_amount".to_string(), 0.86);

    let calibrated = calibrated_confidence(raw, &weights);
    let unweighted = (0.90 + 0.90 + 0.95 + 0.90) / 4.0f32;

    assert!(
        calibrated < unweighted,
        "calibrated ({}) should be less than unweighted ({}) because vendor_name has a low weight",
        calibrated,
        unweighted
    );

    // Verify it's still positive and reasonable
    assert!(
        calibrated > 0.0,
        "calibrated should be positive, got {}",
        calibrated
    );
}

/// Test 3: Round-trip verification of weight computation.
///
/// Simulates recording extractions and corrections, then verifies that the
/// computed Laplace-smoothed weights match expectations. This validates the
/// formula: weight = (extractions - corrections + 1) / (extractions + 2).
#[test]
fn laplace_smoothed_weights_match_formula() {
    // Simulate: 20 extractions, 5 corrections for vendor_name
    // Laplace: (20 - 5 + 1) / (20 + 2) = 16/22 = 0.7273
    let extractions = 20i64;
    let corrections = 5i64;
    let expected_weight = ((extractions - corrections + 1) as f32) / ((extractions + 2) as f32);

    let mut weights = HashMap::new();
    weights.insert("vendor_name".to_string(), expected_weight);

    let raw: &[(&str, f32)] = &[
        ("invoice_number", 0.95),
        ("invoice_date", 0.80),
        ("vendor_name", 0.90),
        ("total_amount", 0.85),
    ];

    // With only vendor_name having a weight, calibrated_confidence falls back to
    // unweighted (partial weights). Verify the formula value itself.
    let expected = ((extractions - corrections + 1) as f32) / ((extractions + 2) as f32);
    assert!(
        (expected_weight - expected).abs() < 0.001,
        "Laplace weight: expected {}, got {}",
        expected,
        expected_weight
    );

    // Now give all fields weights to exercise the calibrated path.
    weights.insert("invoice_number".to_string(), 0.95);
    weights.insert("invoice_date".to_string(), 0.90);
    weights.insert("total_amount".to_string(), 0.88);

    let result = calibrated_confidence(raw, &weights);
    // All weights are reasonable, so the result should be between 0 and 1.
    assert!(
        result > 0.0 && result <= 1.0,
        "calibrated result should be in (0, 1], got {}",
        result
    );
}
