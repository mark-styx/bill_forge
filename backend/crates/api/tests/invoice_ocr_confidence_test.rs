//! Unit tests for the OCR-confidence submission gate.
//!
//! The `submit_for_processing` handler blocks invoices whose `ocr_confidence`
//! is below the shared 0.90 threshold (or missing). These tests exercise the
//! predicate inline so we get coverage without a running database.

use billforge_invoice_capture::OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD;

/// Mirrors the match-arm in `submit_for_processing`:
///
/// ```ignore
/// match invoice.ocr_confidence {
///     Some(conf) if conf >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD => {}
///     _ => return Err(…),
/// }
/// ```
fn ocr_gate_passes(ocr_confidence: Option<f32>) -> bool {
    matches!(
        ocr_confidence,
        Some(conf) if conf >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD
    )
}

#[test]
fn submit_rejects_low_ocr_confidence() {
    // 0.85 is below the 0.90 threshold → should be rejected
    assert!(
        !ocr_gate_passes(Some(0.85)),
        "OCR confidence 0.85 should not pass the 0.90 gate"
    );
}

#[test]
fn submit_rejects_missing_ocr_confidence() {
    // None (no OCR confidence recorded) → should be rejected
    assert!(
        !ocr_gate_passes(None),
        "Missing OCR confidence should not pass the gate"
    );
}

#[test]
fn submit_allows_high_ocr_confidence() {
    // 0.95 is above the 0.90 threshold → should pass
    assert!(
        ocr_gate_passes(Some(0.95)),
        "OCR confidence 0.95 should pass the 0.90 gate"
    );
}

#[test]
fn submit_allows_exact_threshold_confidence() {
    // Exactly 0.90 → should pass (>=)
    assert!(
        ocr_gate_passes(Some(0.90)),
        "OCR confidence exactly at 0.90 threshold should pass"
    );
}
