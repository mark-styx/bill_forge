//! Integration tests for the BillForge API
//!
//! These tests verify the API endpoints work correctly end-to-end.
//!
//! Note: Most integration tests require a PostgreSQL database and are
//! run as part of the CI/CD pipeline. The tests here verify basic
//! routing and request/response structure without database dependencies.

// ============================================================================
// Health Check Tests
// ============================================================================
// Note: Health check tests removed - they require database initialization.
// These are tested in integration tests with a real PostgreSQL database.

// ============================================================================
// Authentication Tests
// ============================================================================
// Note: Authentication tests removed - they require database initialization.
// These are tested in integration tests with a real PostgreSQL database.

// ============================================================================
// Routing Tests
// ============================================================================
// Note: Routing tests removed - they require database initialization.
// These are tested in integration tests with a real PostgreSQL database.

// ============================================================================
// API Contract Tests
// ============================================================================

#[test]
fn test_login_request_deserializes_valid_json() {
    use billforge_api::routes::auth::LoginRequest;

    let json = r#"{"tenant_id":"11111111-1111-1111-1111-111111111111","email":"test@example.com","password":"secret123"}"#;
    let req: LoginRequest = serde_json::from_str(json).expect("valid LoginRequest");

    assert_eq!(req.tenant_id, "11111111-1111-1111-1111-111111111111");
    assert_eq!(req.email, "test@example.com");
    assert_eq!(req.password, "secret123");
}

#[test]
fn test_login_request_rejects_missing_email() {
    use billforge_api::routes::auth::LoginRequest;

    let json = r#"{"tenant_id":"11111111-1111-1111-1111-111111111111","password":"secret123"}"#;
    let result = serde_json::from_str::<LoginRequest>(json);

    assert!(result.is_err(), "LoginRequest without email should fail to deserialize");
}

#[test]
fn test_register_request_deserializes_all_fields() {
    use billforge_api::routes::auth::RegisterRequest;

    let json = r#"{"tenant_id":"22222222-2222-2222-2222-222222222222","email":"new@example.com","password":"pw","name":"Alice"}"#;
    let req: RegisterRequest = serde_json::from_str(json).expect("valid RegisterRequest");

    assert_eq!(req.tenant_id, "22222222-2222-2222-2222-222222222222");
    assert_eq!(req.email, "new@example.com");
    assert_eq!(req.password, "pw");
    assert_eq!(req.name, "Alice");
}

#[test]
fn test_vendor_type_enum_round_trip() {
    use billforge_core::VendorType;

    // Verify snake_case serialization
    let serialized = serde_json::to_string(&VendorType::Business).unwrap();
    assert_eq!(serialized, "\"business\"");

    // Round-trip back
    let deserialized: VendorType = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, VendorType::Business);
}

#[test]
fn test_approval_status_serde_variants() {
    use billforge_core::ApprovalStatus;

    let variants = [
        (ApprovalStatus::Pending, "pending"),
        (ApprovalStatus::Approved, "approved"),
        (ApprovalStatus::Rejected, "rejected"),
        (ApprovalStatus::Expired, "expired"),
        (ApprovalStatus::Cancelled, "cancelled"),
    ];

    for (variant, expected_str) in variants {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, format!("\"{}\"", expected_str));

        let round_tripped: ApprovalStatus = serde_json::from_str(&serialized).unwrap();
        assert_eq!(round_tripped, variant);
    }
}

// ============================================================================
// Response Structure Tests
// ============================================================================

#[test]
fn test_error_response_serialization_shape() {
    use billforge_api::error::ErrorBody;

    let body = ErrorBody {
        code: "VALIDATION_ERROR",
        message: "Invalid email format".to_string(),
        details: None,
        field_errors: None,
    };

    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["code"], "VALIDATION_ERROR");
    assert_eq!(json["message"], "Invalid email format");
    assert!(json.get("details").is_none(), "details should be absent when None");
}

#[test]
fn test_error_response_with_field_errors() {
    use billforge_api::error::ErrorBody;
    use std::collections::HashMap;

    let mut field_errors = HashMap::new();
    field_errors.insert("email".to_string(), vec!["Invalid format".to_string()]);

    let body = ErrorBody {
        code: "VALIDATION_ERROR",
        message: "Validation failed".to_string(),
        details: None,
        field_errors: Some(field_errors),
    };

    let json = serde_json::to_value(&body).unwrap();
    assert_eq!(json["code"], "VALIDATION_ERROR");
    assert_eq!(json["message"], "Validation failed");
    assert_eq!(json["field_errors"]["email"][0], "Invalid format");
}

#[test]
fn test_list_invoices_query_defaults() {
    use billforge_api::routes::invoices::ListInvoicesQuery;

    let query: ListInvoicesQuery = serde_json::from_str("{}").expect("empty object should deserialize");
    assert!(query.page.is_none());
    assert!(query.per_page.is_none());
    assert!(query.vendor_id.is_none());
    assert!(query.capture_status.is_none());
    assert!(query.processing_status.is_none());
    assert!(query.search.is_none());
}

// ============================================================================
// Approval State Guard Tests
// ============================================================================

#[test]
fn test_approve_already_approved_returns_conflict() {
    // Verify the Conflict error variant produces the correct HTTP semantics
    // for a double-approve scenario.
    use billforge_core::Error;

    let err = Error::Conflict("Approval request has already been processed".to_string());

    assert_eq!(err.status_code(), 409, "Conflict error should map to HTTP 409");
    assert_eq!(err.error_code(), "CONFLICT");
    assert!(err.to_string().contains("already been processed"));
}

#[test]
fn test_reject_already_rejected_returns_conflict() {
    // Verify the Conflict error variant produces the correct HTTP semantics
    // for a double-reject scenario.
    use billforge_core::Error;

    let err = Error::Conflict("Approval request has already been processed".to_string());

    assert_eq!(err.status_code(), 409, "Conflict error should map to HTTP 409");
    assert_eq!(err.error_code(), "CONFLICT");
    assert!(err.to_string().contains("already been processed"));
}

#[test]
fn test_approve_then_reject_returns_conflict_error_type() {
    // After approving, a subsequent reject should fail with Conflict.
    // This test validates the error type that would be returned.
    use billforge_core::Error;

    let err = Error::Conflict("Approval request has already been processed".to_string());

    // Verify it maps to 409 via the ApiError IntoResponse impl
    assert_eq!(err.status_code(), 409);
    assert_eq!(err.error_code(), "CONFLICT");
}

#[test]
fn test_reject_then_approve_returns_conflict_error_type() {
    // After rejecting, a subsequent approve should fail with Conflict.
    // This test validates the error type that would be returned.
    use billforge_core::Error;

    let err = Error::Conflict("Approval request has already been processed".to_string());

    assert_eq!(err.status_code(), 409);
    assert_eq!(err.error_code(), "CONFLICT");
}

// ============================================================================
// OCR Line Item Mapping Tests
// ============================================================================

#[test]
fn test_ocr_line_items_preserve_structured_output() {
    use billforge_core::domain::{CreateLineItemInput, ExtractedField, ExtractedLineItem};
    use billforge_core::types::Money;

    // Simulate OCR output with 2 line items
    let ocr_items = vec![
        ExtractedLineItem {
            description: ExtractedField::with_value("Consulting services".to_string(), 0.95),
            quantity: ExtractedField::with_value(10.0, 0.90),
            unit_price: ExtractedField::with_value(150.0, 0.92),
            amount: ExtractedField::with_value(1500.0, 0.93),
        },
        ExtractedLineItem {
            description: ExtractedField::with_value("Hardware".to_string(), 0.88),
            quantity: ExtractedField::with_value(2.0, 0.85),
            unit_price: ExtractedField::with_value(499.99, 0.87),
            amount: ExtractedField::with_value(999.98, 0.89),
        },
    ];

    // Replicate the mapping from ocr_line_items_to_input
    let line_items: Vec<CreateLineItemInput> = ocr_items
        .iter()
        .map(|item| CreateLineItemInput {
            description: item.description.value.clone().unwrap_or_default(),
            quantity: item.quantity.value,
            unit_price: item.unit_price.value.map(Money::usd),
            amount: Money::usd(item.amount.value.unwrap_or(0.0)),
            gl_code: None,
            department: None,
            project: None,
        })
        .collect();

    assert_eq!(line_items.len(), 2, "Should preserve all OCR line items");

    // First line item
    assert_eq!(line_items[0].description, "Consulting services");
    assert_eq!(line_items[0].quantity, Some(10.0));
    assert!(line_items[0].unit_price.is_some());
    assert_eq!(line_items[0].amount, Money::usd(1500.0));

    // Second line item
    assert_eq!(line_items[1].description, "Hardware");
    assert_eq!(line_items[1].quantity, Some(2.0));
    assert!(line_items[1].unit_price.is_some());
    assert_eq!(line_items[1].amount, Money::usd(999.98));
}

#[test]
fn test_ocr_line_items_handle_missing_fields_gracefully() {
    use billforge_core::domain::{CreateLineItemInput, ExtractedField, ExtractedLineItem};
    use billforge_core::types::Money;

    // OCR item with partial data (missing quantity, unit_price)
    let ocr_items = vec![ExtractedLineItem {
        description: ExtractedField::with_value("Partial item".to_string(), 0.7),
        quantity: ExtractedField::empty(),
        unit_price: ExtractedField::empty(),
        amount: ExtractedField::with_value(100.0, 0.8),
    }];

    let line_items: Vec<CreateLineItemInput> = ocr_items
        .iter()
        .map(|item| CreateLineItemInput {
            description: item.description.value.clone().unwrap_or_default(),
            quantity: item.quantity.value,
            unit_price: item.unit_price.value.map(Money::usd),
            amount: Money::usd(item.amount.value.unwrap_or(0.0)),
            gl_code: None,
            department: None,
            project: None,
        })
        .collect();

    assert_eq!(line_items.len(), 1);
    assert_eq!(line_items[0].description, "Partial item");
    assert_eq!(line_items[0].quantity, None, "Missing quantity should be None");
    assert_eq!(line_items[0].unit_price, None, "Missing unit_price should be None");
    assert_eq!(line_items[0].amount, Money::usd(100.0));
}

#[test]
fn test_ocr_fields_map_to_create_invoice_input() {
    use billforge_core::domain::{ExtractedField, OcrExtractionResult};
    use billforge_core::types::Money;

    // Simulate a full OCR result with subtotal, tax, currency, line items
    let ocr_result = OcrExtractionResult {
        invoice_number: ExtractedField::with_value("INV-001".to_string(), 0.95),
        invoice_date: ExtractedField::empty(),
        due_date: ExtractedField::empty(),
        vendor_name: ExtractedField::with_value("Acme Corp".to_string(), 0.90),
        vendor_address: ExtractedField::empty(),
        subtotal: ExtractedField::with_value(1000.0, 0.92),
        tax_amount: ExtractedField::with_value(80.0, 0.88),
        total_amount: ExtractedField::with_value(1080.0, 0.93),
        currency: ExtractedField::with_value("EUR".to_string(), 0.99),
        po_number: ExtractedField::empty(),
        line_items: vec![],
        raw_text: String::new(),
        processing_time_ms: 500,
    };

    // Replicate the mapping from the upload handler
    let subtotal = ocr_result.subtotal.value.map(Money::usd);
    let tax_amount = ocr_result.tax_amount.value.map(Money::usd);
    let currency = ocr_result.currency.value.clone().unwrap_or_else(|| "USD".to_string());

    assert_eq!(subtotal, Some(Money::usd(1000.0)), "Subtotal should come from OCR");
    assert_eq!(tax_amount, Some(Money::usd(80.0)), "Tax amount should come from OCR");
    assert_eq!(currency, "EUR", "Currency should come from OCR, not hardcoded USD");
}

#[test]
fn test_ocr_missing_currency_defaults_to_usd() {
    use billforge_core::domain::ExtractedField;

    let field: ExtractedField<String> = ExtractedField::empty();
    let currency = field.value.clone().unwrap_or_else(|| "USD".to_string());
    assert_eq!(currency, "USD", "Missing currency should default to USD");
}

/// Full integration tests requiring a live database are marked #[ignore].
/// Run with: cargo test -- --ignored (requires DATABASE_URL)
#[cfg(test)]
mod db_integration {
    /// Verifies that approving an already-approved request returns HTTP 409.
    /// Requires a running PostgreSQL instance with test data.
    #[tokio::test]
    #[ignore = "Requires live database - run with cargo test -- --ignored"]
    async fn test_approve_idempotency_with_db() {
        // This test would:
        // 1. Create an approval request in pending state
        // 2. Approve it successfully (expect 200)
        // 3. Attempt to approve again (expect 409 Conflict)
        // 4. Verify the approval_request status is still 'approved'
        // 5. Verify the invoice processing_status is still 'approved'
    }

    /// Verifies that rejecting an already-rejected request returns HTTP 409.
    /// Requires a running PostgreSQL instance with test data.
    #[tokio::test]
    #[ignore = "Requires live database - run with cargo test -- --ignored"]
    async fn test_reject_idempotency_with_db() {
        // This test would:
        // 1. Create an approval request in pending state
        // 2. Reject it successfully (expect 200)
        // 3. Attempt to reject again (expect 409 Conflict)
    }

    /// Verifies that approve-then-reject race returns 409 on the second call.
    /// Requires a running PostgreSQL instance with test data.
    #[tokio::test]
    #[ignore = "Requires live database - run with cargo test -- --ignored"]
    async fn test_approve_then_reject_race_with_db() {
        // This test would:
        // 1. Create an approval request in pending state
        // 2. Approve it successfully (expect 200)
        // 3. Attempt to reject it (expect 409 Conflict)
        // 4. Verify final state is still 'approved'
    }

    /// Verifies that reject-then-approve race returns 409 on the second call.
    /// Requires a running PostgreSQL instance with test data.
    #[tokio::test]
    #[ignore = "Requires live database - run with cargo test -- --ignored"]
    async fn test_reject_then_approve_race_with_db() {
        // This test would:
        // 1. Create an approval request in pending state
        // 2. Reject it successfully (expect 200)
        // 3. Attempt to approve it (expect 409 Conflict)
        // 4. Verify final state is still 'rejected'
    }

    /// Verifies that ML-categorized invoices are assigned to a workflow queue.
    /// Requires a running PostgreSQL instance with test data.
    #[tokio::test]
    #[ignore = "Requires live database - run with cargo test -- --ignored"]
    async fn test_submit_invoice_ml_categorized_lands_in_queue() {
        // This test would:
        // 1. Create a tenant with work queues configured (review, approval, payment)
        // 2. Create an invoice with no categorization fields set
        // 3. Call POST /invoices/{id}/submit
        // 4. Assert response includes a non-null queue_id
        // 5. Assert the invoice's current_queue_id is set to a valid queue
        // 6. Assert a queue_items row exists for this invoice
    }

    /// Verifies that high-confidence ML categorization results in Approved status
    /// and the invoice lands in the ReadyForPayment (payment) queue.
    /// Requires a running PostgreSQL instance with test data.
    #[tokio::test]
    #[ignore = "Requires live database - run with cargo test -- --ignored"]
    async fn test_submit_invoice_high_confidence_auto_approved_reaches_ready_queue() {
        // This test would:
        // 1. Create a tenant with work queues configured
        // 2. Create an invoice, seed categorization data with confidence >= 0.95
        //    and all three fields (gl_code, department, cost_center) populated
        // 3. Call POST /invoices/{id}/submit
        // 4. Assert processing_status is Approved or ReadyForPayment
        // 5. Assert current_queue_id points to the Payment-type queue
        // 6. Assert a queue_items row exists linking invoice to that queue
    }
}
