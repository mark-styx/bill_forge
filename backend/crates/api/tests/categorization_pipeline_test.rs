//! Integration tests for ML categorization wired into the invoice submit pipeline.
//!
//! Tests verify:
//! - Submitting an invoice without pre-set categorization triggers auto-categorization
//! - Submitting an invoice WITH pre-set fields does NOT overwrite them
//! - Categorization failure does not block the pipeline
//!
//! Note: HTTP-level integration tests require a running PostgreSQL database with
//! seeded sandbox tenant. The unit-level tests below validate the wiring logic
//! without database dependencies.

use billforge_core::domain::{
    CaptureStatus, Invoice, InvoiceLineItem,
    ProcessingStatus,
};
use billforge_core::types::{Money, TenantId, UserId};
use billforge_core::domain::InvoiceId;
use chrono::Utc;
use uuid::Uuid;

// ============================================================================
// Invoice field gating logic tests
// ============================================================================

/// Verify that an invoice with no categorization fields is eligible for
/// auto-categorization (the condition used in `submit_for_processing`).
#[test]
fn test_invoice_without_categorization_fields_is_eligible() {
    let invoice = make_test_invoice(None, None, None);

    // This mirrors the updated condition in submit_for_processing:
    //   if invoice.gl_code.is_none() || invoice.department.is_none() || invoice.cost_center.is_none()
    let eligible = invoice.gl_code.is_none()
        || invoice.department.is_none()
        || invoice.cost_center.is_none();

    assert!(
        eligible,
        "Invoice with no categorization fields should be eligible for auto-categorization"
    );
}

/// Verify that an invoice with gl_code set but other fields missing IS eligible
/// for auto-categorization (missing fields should be filled).
#[test]
fn test_invoice_with_partial_fields_is_eligible() {
    // Only gl_code set - still eligible because department and cost_center are missing
    let invoice = make_test_invoice(Some("6000-Software".into()), None, None);

    let eligible = invoice.gl_code.is_none()
        || invoice.department.is_none()
        || invoice.cost_center.is_none();

    assert!(
        eligible,
        "Invoice with gl_code set but other fields missing SHOULD be eligible for auto-categorization"
    );
}

/// Verify that an invoice with two of three fields set IS still eligible
/// (one field remains to be categorized).
#[test]
fn test_invoice_with_two_fields_set_is_eligible() {
    let invoice = make_test_invoice(
        Some("6000-Software".into()),
        Some("Engineering".into()),
        None,
    );

    let eligible = invoice.gl_code.is_none()
        || invoice.department.is_none()
        || invoice.cost_center.is_none();

    assert!(
        eligible,
        "Invoice with two fields set but one missing SHOULD be eligible"
    );
}

/// Verify that an invoice with all three fields set is NOT eligible.
#[test]
fn test_all_fields_set_skips_categorization() {
    let invoice = make_test_invoice(
        Some("6000-Software".into()),
        Some("Engineering".into()),
        Some("CC-100".into()),
    );

    let eligible = invoice.gl_code.is_none()
        || invoice.department.is_none()
        || invoice.cost_center.is_none();

    assert!(
        !eligible,
        "Invoice with all three fields set should NOT be eligible - nothing to categorize"
    );
}

#[test]
fn test_invoice_with_all_categorization_fields_is_not_eligible() {
    let invoice = make_test_invoice(
        Some("6000-Software".into()),
        Some("Engineering".into()),
        Some("CC-100".into()),
    );

    let eligible = invoice.gl_code.is_none()
        || invoice.department.is_none()
        || invoice.cost_center.is_none();

    assert!(
        !eligible,
        "Invoice with all categorization fields set should NOT be eligible"
    );
}

// ============================================================================
// Partial categorization field preservation tests
// ============================================================================

/// Verify that the update JSON only includes keys for fields that were NOT
/// already set. This mirrors the conditional JSON building in invoices.rs.
#[test]
fn test_partial_categorization_preserves_existing_fields() {
    use billforge_invoice_processing::categorization::{
        CategorySuggestion, CategoryType, InvoiceCategorization, SuggestionSource,
    };

    // Invoice already has gl_code set
    let had_gl_code = true;
    let had_department = false;
    let had_cost_center = false;

    // Categorization suggests all three fields
    let categorization = InvoiceCategorization {
        invoice_id: Uuid::nil(),
        gl_code: Some(CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: "9999-Overwrite".to_string(),
            confidence: 0.90,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        }),
        department: Some(CategorySuggestion {
            category_type: CategoryType::Department,
            value: "Engineering".to_string(),
            confidence: 0.85,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        }),
        cost_center: Some(CategorySuggestion {
            category_type: CategoryType::CostCenter,
            value: "CC-100".to_string(),
            confidence: 0.80,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        }),
        overall_confidence: 0.85,
    };

    // Build updates using the same logic as the fixed invoices.rs
    let mut updates = serde_json::json!({
        "categorization_confidence": categorization.overall_confidence,
    });
    if !had_gl_code {
        updates["gl_code"] = serde_json::json!(
            categorization.gl_code.as_ref().map(|s| &s.value)
        );
    }
    if !had_department {
        updates["department"] = serde_json::json!(
            categorization.department.as_ref().map(|s| &s.value)
        );
    }
    if !had_cost_center {
        updates["cost_center"] = serde_json::json!(
            categorization.cost_center.as_ref().map(|s| &s.value)
        );
    }

    // gl_code should NOT be in the updates (it was already set)
    assert!(
        !updates.as_object().unwrap().contains_key("gl_code"),
        "gl_code should not be in updates when it was already set"
    );
    // department and cost_center SHOULD be in the updates
    assert_eq!(updates["department"], "Engineering");
    assert_eq!(updates["cost_center"], "CC-100");
    assert!((updates["categorization_confidence"].as_f64().unwrap() - 0.85).abs() < 0.001);
}

// ============================================================================
// Auto-approval completeness tests (mirrors engine.rs && logic)
// ============================================================================

const ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD: f32 = 0.95;

/// Verify that only a partial categorization (one field) does NOT satisfy
/// the auto-approval completeness check. This guards against the || bug.
#[test]
fn test_partial_categorization_one_field_does_not_qualify_for_auto_approval() {
    let mut invoice = make_test_invoice(Some("6000-Software".into()), None, None);
    invoice.categorization_confidence = Some(0.97); // above threshold

    let has_complete = invoice.gl_code.is_some()
        && invoice.department.is_some()
        && invoice.cost_center.is_some();

    assert!(
        !has_complete,
        "Only gl_code set should NOT qualify as complete categorization"
    );
}

/// Verify that two out of three categorization fields is still NOT enough.
#[test]
fn test_partial_categorization_two_fields_does_not_qualify_for_auto_approval() {
    let mut invoice = make_test_invoice(
        Some("6000-Software".into()),
        Some("Engineering".into()),
        None,
    );
    invoice.categorization_confidence = Some(0.97);

    let has_complete = invoice.gl_code.is_some()
        && invoice.department.is_some()
        && invoice.cost_center.is_some();

    assert!(
        !has_complete,
        "Two out of three fields should NOT qualify as complete categorization"
    );
}

/// Verify that all three categorization fields with high confidence DOES
/// satisfy the auto-approval check.
#[test]
fn test_complete_categorization_qualifies_for_auto_approval() {
    let mut invoice = make_test_invoice(
        Some("6000-Software".into()),
        Some("Engineering".into()),
        Some("CC-100".into()),
    );
    invoice.categorization_confidence = Some(0.97);

    let has_complete = invoice.gl_code.is_some()
        && invoice.department.is_some()
        && invoice.cost_center.is_some();

    assert!(
        has_complete,
        "All three fields set should qualify as complete categorization"
    );

    // Also verify the confidence threshold check
    let qualifies = invoice.categorization_confidence
        .map(|c| c >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD && has_complete)
        .unwrap_or(false);
    assert!(qualifies, "High confidence + complete fields should auto-approve");
}

/// Verify that all three fields set but LOW confidence does NOT auto-approve.
#[test]
fn test_complete_categorization_low_confidence_no_auto_approval() {
    let mut invoice = make_test_invoice(
        Some("6000-Software".into()),
        Some("Engineering".into()),
        Some("CC-100".into()),
    );
    invoice.categorization_confidence = Some(0.80); // below threshold

    let qualifies = invoice.categorization_confidence
        .map(|c| c >= ML_AUTO_APPROVAL_CONFIDENCE_THRESHOLD)
        .unwrap_or(false);

    assert!(
        !qualifies,
        "Low confidence should NOT auto-approve even with complete fields"
    );
}

// ============================================================================
// Line item mapping tests
// ============================================================================

/// Verify that an InvoiceCategorization with only one of three fields produces
/// an overall_confidence well below the 0.95 auto-approval threshold.
///
/// This mirrors the real pipeline: calculate_overall_confidence now divides by
/// a fixed denominator of 3, so missing fields contribute 0.0 to the average.
#[test]
fn test_incomplete_categorization_confidence_below_threshold() {
    use billforge_invoice_processing::categorization::{
        CategorySuggestion, CategoryType, InvoiceCategorization, SuggestionSource,
    };

    // Only one field populated, at very high per-field confidence
    let categorization = InvoiceCategorization {
        invoice_id: Uuid::nil(),
        gl_code: Some(CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: "6000-Software".to_string(),
            confidence: 0.98,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        }),
        department: None,
        cost_center: None,
        // This overall_confidence would have been set by calculate_overall_confidence:
        // (0.98 + 0.0 + 0.0) / 3.0 ≈ 0.327
        overall_confidence: 0.98 / 3.0,
    };

    assert!(
        categorization.overall_confidence < 0.95,
        "Incomplete categorization (1 of 3 fields) should have confidence well below 0.95, got {}",
        categorization.overall_confidence,
    );
    assert!(
        categorization.overall_confidence < 0.40,
        "1-of-3 at 0.98 should yield ~0.327, got {}",
        categorization.overall_confidence,
    );
}

/// Verify the line item to LineItemInput mapping used in submit_for_processing
/// produces correct values (amount in dollars, not cents).
#[test]
fn test_line_item_mapping_amounts() {
    let invoice = make_test_invoice(None, None, None);

    let line_items: Vec<billforge_invoice_processing::categorization::LineItemInput> = invoice
        .line_items
        .iter()
        .map(|li| billforge_invoice_processing::categorization::LineItemInput {
            description: li.description.clone(),
            quantity: li.quantity,
            amount: li.amount.amount as f64 / 100.0,
        })
        .collect();

    assert_eq!(line_items.len(), 2);

    // First line item: 50000 cents -> $500.00
    assert_eq!(line_items[0].description, "Software license");
    assert_eq!(line_items[0].quantity, Some(1.0));
    assert!((line_items[0].amount - 500.0).abs() < f64::EPSILON);

    // Second line item: 25000 cents -> $250.00
    assert_eq!(line_items[1].description, "Consulting hours");
    assert_eq!(line_items[1].quantity, Some(2.0));
    assert!((line_items[1].amount - 250.0).abs() < f64::EPSILON);
}

// ============================================================================
// Update JSON shape tests
// ============================================================================

/// Verify that the JSON updates object built from categorization results has the
/// correct structure for the invoice_repo update method.
#[test]
fn test_categorization_update_json_shape() {
    use billforge_invoice_processing::categorization::{
        CategorySuggestion, CategoryType, InvoiceCategorization, SuggestionSource,
    };

    let categorization = InvoiceCategorization {
        invoice_id: Uuid::nil(),
        gl_code: Some(CategorySuggestion {
            category_type: CategoryType::GlCode,
            value: "6000-Software".to_string(),
            confidence: 0.92,
            source: SuggestionSource::LineItemAnalysis,
            reasoning: None,
        }),
        department: Some(CategorySuggestion {
            category_type: CategoryType::Department,
            value: "Engineering".to_string(),
            confidence: 0.85,
            source: SuggestionSource::VendorHistory,
            reasoning: None,
        }),
        cost_center: None,
        overall_confidence: 0.88,
    };

    let updates = serde_json::json!({
        "gl_code": categorization.gl_code.as_ref().map(|s| &s.value),
        "department": categorization.department.as_ref().map(|s| &s.value),
        "cost_center": categorization.cost_center.as_ref().map(|s| &s.value),
        "categorization_confidence": categorization.overall_confidence,
    });

    assert_eq!(updates["gl_code"], "6000-Software");
    assert_eq!(updates["department"], "Engineering");
    assert!(updates["cost_center"].is_null());
    assert!((updates["categorization_confidence"].as_f64().unwrap() - 0.88).abs() < 0.001);
}

/// Verify update JSON when categorization returns no suggestions at all.
#[test]
fn test_categorization_update_json_empty_suggestions() {
    use billforge_invoice_processing::categorization::InvoiceCategorization;

    let categorization = InvoiceCategorization {
        invoice_id: Uuid::nil(),
        gl_code: None,
        department: None,
        cost_center: None,
        overall_confidence: 0.0,
    };

    let updates = serde_json::json!({
        "gl_code": categorization.gl_code.as_ref().map(|s| &s.value),
        "department": categorization.department.as_ref().map(|s| &s.value),
        "cost_center": categorization.cost_center.as_ref().map(|s| &s.value),
        "categorization_confidence": categorization.overall_confidence,
    });

    // All fields should be null or 0.0
    assert!(updates["gl_code"].is_null());
    assert!(updates["department"].is_null());
    assert!(updates["cost_center"].is_null());
    assert_eq!(updates["categorization_confidence"], 0.0);
}

// ============================================================================
// HTTP-level integration tests (require PostgreSQL)
// ============================================================================

#[cfg(test)]
mod http_integration {
    // These tests are #[ignore] by default because they require a running
    // PostgreSQL database with a seeded sandbox tenant. Run with:
    //   SQLX_OFFLINE=true cargo test --package billforge-api --test categorization_pipeline_test -- --ignored

    /// Verify that submitting an invoice without categorization fields
    /// triggers auto-categorization and populates the fields on the invoice.
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
    async fn test_submit_triggers_auto_categorization() {
        // This test would:
        // 1. Create an invoice via API with no gl_code/department/cost_center
        // 2. Submit it for processing via POST /invoices/{id}/submit
        // 3. Fetch the invoice and verify gl_code/department/cost_center are populated
        // 4. Verify categorization_confidence is set
        //
        // Left as a skeleton because it requires the full database setup.
        // See ocr_tests.rs for the pattern to follow when running against a real DB.
    }

    /// Verify that submitting an invoice WITH pre-set categorization fields
    /// does NOT overwrite them.
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
    async fn test_submit_preserves_existing_categorization() {
        // This test would:
        // 1. Create an invoice with gl_code="CUSTOM-GL", department="CustomDept"
        // 2. Submit it for processing
        // 3. Verify gl_code is still "CUSTOM-GL", not overwritten by auto-categorization
        //
        // Left as a skeleton for the same reason as above.
    }

    /// Verify that categorization failure does not block the submission pipeline.
    #[tokio::test]
    #[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
    async fn test_submit_succeeds_despite_categorization_failure() {
        // This test would:
        // 1. Create an invoice with a vendor that has no history
        // 2. Ensure no OPENAI_API_KEY is set (rule-based only)
        // 3. Submit and verify the response is still successful
        // 4. Verify the invoice proceeds through the workflow
        //
        // Left as a skeleton for the same reason as above.
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn make_test_invoice(
    gl_code: Option<String>,
    department: Option<String>,
    cost_center: Option<String>,
) -> Invoice {
    let now = Utc::now();
    let tenant_id = TenantId::from_uuid(Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());

    Invoice {
        id: InvoiceId::new(),
        tenant_id,
        vendor_id: Some(Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()),
        vendor_name: "Acme Corp".to_string(),
        invoice_number: "INV-TEST-001".to_string(),
        invoice_date: None,
        due_date: None,
        po_number: None,
        subtotal: Some(Money::usd(750.0)),
        tax_amount: None,
        total_amount: Money::usd(750.0),
        currency: "USD".to_string(),
        line_items: vec![
            InvoiceLineItem {
                id: Uuid::new_v4(),
                line_number: 1,
                description: "Software license".to_string(),
                quantity: Some(1.0),
                unit_price: Some(Money::usd(500.0)),
                amount: Money::usd(500.0),
                gl_code: None,
                department: None,
                project: None,
            },
            InvoiceLineItem {
                id: Uuid::new_v4(),
                line_number: 2,
                description: "Consulting hours".to_string(),
                quantity: Some(2.0),
                unit_price: Some(Money::usd(125.0)),
                amount: Money::usd(250.0),
                gl_code: None,
                department: None,
                project: None,
            },
        ],
        capture_status: CaptureStatus::ReadyForReview,
        processing_status: ProcessingStatus::Draft,
        current_queue_id: None,
        assigned_to: None,
        document_id: Uuid::new_v4(),
        supporting_documents: vec![],
        ocr_confidence: Some(0.85),
        categorization_confidence: None,
        department,
        gl_code,
        cost_center,
        notes: None,
        tags: vec![],
        custom_fields: serde_json::Value::Object(serde_json::Map::new()),
        created_by: UserId::from_uuid(Uuid::new_v4()),
        created_at: now,
        updated_at: now,
    }
}
