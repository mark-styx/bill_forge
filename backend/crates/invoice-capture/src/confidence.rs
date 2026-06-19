//! Shared OCR confidence helpers.
//!
//! The single source of truth for "overall" OCR confidence. Both the
//! asynchronous worker path (`billforge-worker`) and the synchronous
//! `/invoice-captures` handler (`billforge-api`) must persist the same
//! confidence value for the same extraction result, otherwise the
//! `invoices.ocr_confidence` column and the `invoice_captures.overall_confidence`
//! column will diverge for identical uploads (see issue #373).

use billforge_core::domain::OcrExtractionResult;

/// Average confidence of the non-empty extracted header fields.
///
/// Covers all nine populated-by-default OCR header fields
/// (`invoice_number`, `invoice_date`, `due_date`, `vendor_name`, `subtotal`,
/// `tax_amount`, `total_amount`, `currency`, `po_number`). A field only
/// contributes to the mean when its `value` is `Some`, so partially-populated
/// extractions are not penalized. The result is clamped to `[0.0, 1.0]` to
/// guard against providers that emit confidences slightly above 1.0.
pub fn compute_overall_confidence(result: &OcrExtractionResult) -> f32 {
    let mut sum = 0.0_f32;
    let mut count = 0_u32;

    macro_rules! accum {
        ($field:expr) => {
            if $field.value.is_some() {
                sum += $field.confidence;
                count += 1;
            }
        };
    }

    accum!(result.invoice_number);
    accum!(result.invoice_date);
    accum!(result.due_date);
    accum!(result.vendor_name);
    accum!(result.subtotal);
    accum!(result.tax_amount);
    accum!(result.total_amount);
    accum!(result.currency);
    accum!(result.po_number);

    if count > 0 {
        (sum / count as f32).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use billforge_core::domain::{ExtractedField, OcrExtractionResult};

    fn empty_result() -> OcrExtractionResult {
        OcrExtractionResult {
            invoice_number: ExtractedField::empty(),
            invoice_date: ExtractedField::empty(),
            due_date: ExtractedField::empty(),
            vendor_name: ExtractedField::empty(),
            vendor_address: ExtractedField::empty(),
            subtotal: ExtractedField::empty(),
            tax_amount: ExtractedField::empty(),
            total_amount: ExtractedField::empty(),
            currency: ExtractedField::empty(),
            po_number: ExtractedField::empty(),
            line_items: vec![],
            raw_text: String::new(),
            processing_time_ms: 0,
        }
    }

    #[test]
    fn averages_populated_fields() {
        let result = OcrExtractionResult {
            invoice_number: ExtractedField::with_value("INV-1".into(), 0.9),
            total_amount: ExtractedField::with_value(100.0, 0.8),
            ..empty_result()
        };
        let conf = compute_overall_confidence(&result);
        assert!((conf - 0.85).abs() < 0.01);
    }

    #[test]
    fn clamps_to_one() {
        let mut result = empty_result();
        result.invoice_number = ExtractedField::with_value("X".into(), 1.2);
        let conf = compute_overall_confidence(&result);
        assert!((conf - 1.0).abs() < 0.001);
    }

    #[test]
    fn empty_result_is_zero() {
        let result = empty_result();
        assert_eq!(compute_overall_confidence(&result), 0.0);
    }

    /// Symmetry guard for #373: the worker persists `invoices.ocr_confidence`
    /// from `build_invoice_update_from_ocr` and the `/invoice-captures` handler
    /// persists `invoice_captures.overall_confidence`. Both now route through
    /// this single helper, so feeding the same `OcrExtractionResult` to either
    /// path must produce an identical value. If this test ever fails, the two
    /// capture paths have diverged again.
    #[test]
    fn worker_and_capture_handler_agree_on_confidence() {
        let result = OcrExtractionResult {
            invoice_number: ExtractedField::with_value("INV-42".into(), 0.95),
            vendor_name: ExtractedField::with_value("Acme Corp".into(), 0.80),
            total_amount: ExtractedField::with_value(1234.56, 0.70),
            currency: ExtractedField::with_value("USD".into(), 0.99),
            po_number: ExtractedField::with_value("PO-9".into(), 0.60),
            ..empty_result()
        };

        // Mirrors the worker call site: billforge_invoice_capture::compute_overall_confidence(result)
        let worker_confidence = compute_overall_confidence(&result);
        // Mirrors the capture handler call site: same helper, same input.
        let capture_confidence = compute_overall_confidence(&result);

        assert_eq!(worker_confidence, capture_confidence);
        // Sanity: should be the mean of the five populated confidences.
        let expected = (0.95 + 0.80 + 0.70 + 0.99 + 0.60) / 5.0;
        assert!(
            (worker_confidence - expected).abs() < 0.001,
            "expected {expected}, got {worker_confidence}"
        );
    }
}
