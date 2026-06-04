//! Tests for the worker's OCR comparison path (Issue #329).
//!
//! Exercises `select_from_comparison` which implements the confidence-threshold
//! swap logic — the core of the comparison branch wired into `process_ocr`.

use std::collections::HashMap;

use billforge_core::domain::{ExtractedField, OcrExtractionResult};
use billforge_invoice_capture::ocr::ocr_comparison::{
    ComparisonMetrics, OcrComparisonResult, ProviderResult,
};
use billforge_worker::jobs::ocr_processing::select_from_comparison;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn high_confidence_result() -> OcrExtractionResult {
    OcrExtractionResult {
        invoice_number: ExtractedField::with_value("HIGH-INV".to_string(), 0.95),
        invoice_date: ExtractedField::empty(),
        due_date: ExtractedField::empty(),
        vendor_name: ExtractedField::with_value("HighCorp".to_string(), 0.90),
        vendor_address: ExtractedField::empty(),
        subtotal: ExtractedField::empty(),
        tax_amount: ExtractedField::empty(),
        total_amount: ExtractedField::with_value(500.0, 0.92),
        currency: ExtractedField::with_value("USD".to_string(), 0.99),
        po_number: ExtractedField::empty(),
        line_items: vec![],
        raw_text: String::new(),
        processing_time_ms: 80,
    }
}

fn low_confidence_result() -> OcrExtractionResult {
    OcrExtractionResult {
        invoice_number: ExtractedField::with_value("LOW-INV".to_string(), 0.30),
        invoice_date: ExtractedField::empty(),
        due_date: ExtractedField::empty(),
        vendor_name: ExtractedField::with_value("LowCorp".to_string(), 0.25),
        vendor_address: ExtractedField::empty(),
        subtotal: ExtractedField::empty(),
        tax_amount: ExtractedField::empty(),
        total_amount: ExtractedField::with_value(10.0, 0.28),
        currency: ExtractedField::with_value("USD".to_string(), 0.50),
        po_number: ExtractedField::empty(),
        line_items: vec![],
        raw_text: String::new(),
        processing_time_ms: 50,
    }
}

fn make_provider_result(
    provider: &str,
    result: OcrExtractionResult,
    success: bool,
) -> ProviderResult {
    let confidence = if success {
        // Replicate OcrComparison::calculate_confidence_score roughly
        let mut score = 0.0;
        let mut weight = 0.0;
        if result.invoice_number.value.is_some() {
            score += result.invoice_number.confidence * 2.0;
            weight += 2.0;
        }
        if result.total_amount.value.is_some() {
            score += result.total_amount.confidence * 2.0;
            weight += 2.0;
        }
        if result.vendor_name.value.is_some() {
            score += result.vendor_name.confidence * 1.5;
            weight += 1.5;
        }
        if weight > 0.0 { score / weight } else { 0.0 }
    } else {
        0.0
    };
    ProviderResult {
        provider: provider.to_string(),
        result: if success { Some(result) } else { None },
        processing_time_ms: 100,
        success,
        error: if success { None } else { Some("mock error".to_string()) },
        confidence_score: confidence,
    }
}

fn make_comparison_result(
    primary_name: &str,
    primary_result: OcrExtractionResult,
    primary_success: bool,
    fallback_name: &str,
    fallback_result: OcrExtractionResult,
    fallback_success: bool,
    best_provider: &str,
) -> OcrComparisonResult {
    let primary_pr = make_provider_result(primary_name, primary_result, primary_success);
    let fallback_pr = make_provider_result(fallback_name, fallback_result, fallback_success);

    let mut providers = HashMap::new();
    providers.insert(primary_name.to_string(), primary_pr);
    providers.insert(fallback_name.to_string(), fallback_pr);

    OcrComparisonResult {
        providers,
        best_provider: best_provider.to_string(),
        comparison_metrics: ComparisonMetrics {
            fields_in_agreement: vec![],
            fields_in_conflict: vec![],
            agreement_percentage: 100.0,
        },
    }
}

// ---------------------------------------------------------------------------
// Case A: primary high-confidence, fallback low → comparison picks primary
// ---------------------------------------------------------------------------

#[test]
fn case_a_primary_high_confidence_picks_primary() {
    let cmp = make_comparison_result(
        "tesseract",
        high_confidence_result(),
        true,
        "aws_textract",
        low_confidence_result(),
        true,
        "tesseract",
    );

    let outcome = select_from_comparison(&cmp, "tesseract", "aws_textract", 0.6)
        .expect("should succeed");

    assert_eq!(outcome.selected_provider, "tesseract");
    assert_eq!(
        outcome.extraction.invoice_number.value.as_deref(),
        Some("HIGH-INV")
    );
}

// ---------------------------------------------------------------------------
// Case B: primary low-confidence below threshold, fallback higher → swap
// ---------------------------------------------------------------------------

#[test]
fn case_b_primary_low_confidence_swaps_to_fallback() {
    // Best provider is still primary (by OcrComparison logic), but its
    // confidence is below the 0.6 threshold and the fallback is higher.
    let cmp = make_comparison_result(
        "tesseract",
        low_confidence_result(),
        true,
        "aws_textract",
        high_confidence_result(),
        true,
        "tesseract",
    );

    let outcome = select_from_comparison(&cmp, "tesseract", "aws_textract", 0.6)
        .expect("should succeed");

    assert_eq!(
        outcome.selected_provider, "aws_textract",
        "should swap to fallback because primary confidence < 0.6"
    );
    assert_eq!(
        outcome.extraction.invoice_number.value.as_deref(),
        Some("HIGH-INV"),
        "should contain fallback extraction data"
    );
}

// ---------------------------------------------------------------------------
// Case B variant: fallback picked directly by comparison (best_provider ==
// fallback).  No threshold swap needed — the comparison already chose the
// better provider.
// ---------------------------------------------------------------------------

#[test]
fn case_b_variant_comparison_picks_fallback_directly() {
    let cmp = make_comparison_result(
        "tesseract",
        low_confidence_result(),
        true,
        "aws_textract",
        high_confidence_result(),
        true,
        "aws_textract",
    );

    let outcome = select_from_comparison(&cmp, "tesseract", "aws_textract", 0.6)
        .expect("should succeed");

    assert_eq!(outcome.selected_provider, "aws_textract");
    assert_eq!(
        outcome.extraction.invoice_number.value.as_deref(),
        Some("HIGH-INV")
    );
}

// ---------------------------------------------------------------------------
// Edge: both providers succeed but primary is exactly at threshold — no swap
// ---------------------------------------------------------------------------

#[test]
fn primary_at_threshold_no_swap() {
    let mut at_threshold = high_confidence_result();
    // Lower confidence so weighted score lands around 0.6
    at_threshold.invoice_number = ExtractedField::with_value("MED-INV".to_string(), 0.6);
    at_threshold.vendor_name = ExtractedField::with_value("MedCorp".to_string(), 0.6);
    at_threshold.total_amount = ExtractedField::with_value(100.0, 0.6);

    let cmp = make_comparison_result(
        "tesseract",
        at_threshold,
        true,
        "aws_textract",
        low_confidence_result(),
        true,
        "tesseract",
    );

    let outcome = select_from_comparison(&cmp, "tesseract", "aws_textract", 0.6)
        .expect("should succeed");

    // Primary is exactly at threshold (0.6), not strictly below, so no swap
    assert_eq!(outcome.selected_provider, "tesseract");
}

// ---------------------------------------------------------------------------
// Edge: fallback fails entirely — primary wins even if low confidence
// ---------------------------------------------------------------------------

#[test]
fn fallback_fails_primary_wins_regardless() {
    let cmp = make_comparison_result(
        "tesseract",
        low_confidence_result(),
        true,
        "aws_textract",
        low_confidence_result(), // won't be used
        false,                   // fallback failed
        "tesseract",
    );

    let outcome = select_from_comparison(&cmp, "tesseract", "aws_textract", 0.6)
        .expect("should succeed");

    assert_eq!(outcome.selected_provider, "tesseract");
}

// ---------------------------------------------------------------------------
// Edge: both providers fail — returns error
// ---------------------------------------------------------------------------

#[test]
fn both_providers_fail_returns_error() {
    let cmp = make_comparison_result(
        "tesseract",
        low_confidence_result(),
        false,
        "aws_textract",
        low_confidence_result(),
        false,
        "tesseract",
    );

    let result = select_from_comparison(&cmp, "tesseract", "aws_textract", 0.6);
    assert!(result.is_err());
    assert!(
        result.unwrap_err().contains("All OCR providers failed"),
        "expected all-failed error message"
    );
}

// ---------------------------------------------------------------------------
// Regression: threshold swap respects min_confidence boundary
// ---------------------------------------------------------------------------

#[test]
fn no_swap_when_fallback_confidence_equal_to_primary() {
    // Both have same low confidence. Swap requires fallback_conf > primary_conf.
    let cmp = make_comparison_result(
        "tesseract",
        low_confidence_result(),
        true,
        "aws_textract",
        low_confidence_result(), // same confidence as primary
        true,
        "tesseract",
    );

    let outcome = select_from_comparison(&cmp, "tesseract", "aws_textract", 0.6)
        .expect("should succeed");

    // No swap because fallback_conf is NOT strictly greater than primary_conf
    assert_eq!(outcome.selected_provider, "tesseract");
}
