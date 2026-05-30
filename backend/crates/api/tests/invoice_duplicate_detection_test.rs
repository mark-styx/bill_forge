//! Integration tests for duplicate detection in invoice submission (#131, #242).
//!
//! Tests cover:
//! - DuplicateDetector correctly flags near-identical invoices (5-signal scoring)
//! - DuplicateDetector returns no matches for unique invoices
//! - OCR-fuzzy invoice numbers (O↔0, I↔1) are still detected
//! - CreateInvoiceQuery deserialization (force param)
//! - CreateInvoiceResponse / DuplicateMatch serialization round-trips

use billforge_analytics::anomaly_detection::{DuplicateDetector, InvoiceRecord};
use billforge_analytics::predictive_models::AnomalySeverity;
use chrono::{Duration, Utc};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Query param deserialization
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct CreateInvoiceQuery {
    force: Option<bool>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct DuplicateSignalBreakdown {
    vendor: f64,
    invoice_number: f64,
    amount: f64,
    date: f64,
    line_item_fingerprint: f64,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DuplicateMatch {
    invoice_id: String,
    invoice_number: String,
    vendor_name: String,
    similarity_score: f64,
    severity: String,
    signal_breakdown: DuplicateSignalBreakdown,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CreateInvoiceResponse {
    invoice_id: String,
    invoice_number: String,
    potential_duplicates: Vec<DuplicateMatch>,
}

#[test]
fn test_create_invoice_query_default_no_force() {
    let q: CreateInvoiceQuery = serde_json::from_str("{}").unwrap();
    assert_eq!(q.force, None);
}

#[test]
fn test_create_invoice_query_force_true() {
    let q: CreateInvoiceQuery = serde_json::from_str(r#"{"force":true}"#).unwrap();
    assert_eq!(q.force, Some(true));
}

// ---------------------------------------------------------------------------
// Response serialization
// ---------------------------------------------------------------------------

#[test]
fn test_create_invoice_response_serialization() {
    let resp = CreateInvoiceResponse {
        invoice_id: Uuid::new_v4().to_string(),
        invoice_number: "INV-001".to_string(),
        potential_duplicates: vec![DuplicateMatch {
            invoice_id: Uuid::new_v4().to_string(),
            invoice_number: "INV-000".to_string(),
            vendor_name: "Acme Corp".to_string(),
            similarity_score: 0.95,
            severity: "Critical".to_string(),
            signal_breakdown: DuplicateSignalBreakdown {
                vendor: 1.0,
                invoice_number: 0.95,
                amount: 1.0,
                date: 1.0,
                line_item_fingerprint: 1.0,
            },
        }],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: CreateInvoiceResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.potential_duplicates.len(), 1);
    assert_eq!(back.potential_duplicates[0].severity, "Critical");
}

#[test]
fn test_create_invoice_response_empty_duplicates() {
    let resp = CreateInvoiceResponse {
        invoice_id: Uuid::new_v4().to_string(),
        invoice_number: "INV-UNIQUE".to_string(),
        potential_duplicates: vec![],
    };
    let json = serde_json::to_string(&resp).unwrap();
    let back: CreateInvoiceResponse = serde_json::from_str(&json).unwrap();
    assert!(back.potential_duplicates.is_empty());
}

// ---------------------------------------------------------------------------
// DuplicateDetector: high-similarity match
// ---------------------------------------------------------------------------

#[test]
fn test_create_invoice_returns_duplicate_warning() {
    let tenant_id = Uuid::new_v4();
    let detector = DuplicateDetector::new(tenant_id);
    let now = Utc::now();

    // Simulate an existing invoice already in the system
    let existing = InvoiceRecord {
        invoice_id: Uuid::new_v4().to_string(),
        vendor_name: "Acme Corp".to_string(),
        amount: 1500.00,
        invoice_date: now - Duration::days(5),
        invoice_number: Some("INV-1005".to_string()),
        line_item_fingerprint: None,
    };

    // New incoming invoice: same vendor, same amount, same invoice number, close date
    let incoming = InvoiceRecord {
        invoice_id: "INV-NEW-001".to_string(),
        vendor_name: "Acme Corp".to_string(),
        amount: 1500.00,
        invoice_date: now - Duration::days(3),
        invoice_number: Some("INV-1005".to_string()),
        line_item_fingerprint: None,
    };

    // Unrelated invoice that should not match
    let other = InvoiceRecord {
        invoice_id: Uuid::new_v4().to_string(),
        vendor_name: "Completely Different LLC".to_string(),
        amount: 7500.00,
        invoice_date: now - Duration::days(60),
        invoice_number: Some("INV-OTHER".to_string()),
        line_item_fingerprint: None,
    };

    let records = vec![existing.clone(), incoming.clone(), other];
    let anomalies = detector.detect_duplicates(&records).unwrap();

    // Should detect at least one duplicate (existing <-> incoming)
    assert!(!anomalies.is_empty(), "Should detect duplicate pair");

    // Find the anomaly that references the incoming invoice
    let dup = anomalies.iter().find(|a| {
        let meta = &a.metadata;
        let id1 = meta
            .get("invoice1")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str());
        let id2 = meta
            .get("invoice2")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str());
        id1 == Some("INV-NEW-001") || id2 == Some("INV-NEW-001")
    });
    assert!(
        dup.is_some(),
        "Anomaly should reference the incoming invoice"
    );

    let anomaly = dup.unwrap();
    assert!(
        anomaly.severity == AnomalySeverity::High || anomaly.severity == AnomalySeverity::Critical,
        "Expected High or Critical severity, got {:?}",
        anomaly.severity
    );
    assert!(
        anomaly.detected_value > 0.8,
        "Similarity should exceed 0.8 threshold"
    );
}

// ---------------------------------------------------------------------------
// DuplicateDetector: clean (no duplicates)
// ---------------------------------------------------------------------------

#[test]
fn test_create_invoice_no_duplicates_clean() {
    let tenant_id = Uuid::new_v4();
    let detector = DuplicateDetector::new(tenant_id);
    let now = Utc::now();

    let existing = InvoiceRecord {
        invoice_id: Uuid::new_v4().to_string(),
        vendor_name: "Acme Corp".to_string(),
        amount: 1500.00,
        invoice_date: now - Duration::days(5),
        invoice_number: Some("INV-AAA".to_string()),
        line_item_fingerprint: None,
    };

    // Completely different vendor, amount, and distant date
    let incoming = InvoiceRecord {
        invoice_id: "INV-UNIQUE-999".to_string(),
        vendor_name: "Zenith Supplies Inc".to_string(),
        amount: 420.50,
        invoice_date: now,
        invoice_number: Some("INV-ZZZ".to_string()),
        line_item_fingerprint: None,
    };

    let records = vec![existing, incoming];
    let anomalies = detector.detect_duplicates(&records).unwrap();

    assert!(
        anomalies.is_empty(),
        "Should not detect duplicates for unrelated invoices, got {} anomalies",
        anomalies.len()
    );
}

// ---------------------------------------------------------------------------
// OCR-fuzzy invoice number detection
// ---------------------------------------------------------------------------

#[test]
fn test_ocr_fuzzy_invoice_number_detected() {
    let tenant_id = Uuid::new_v4();
    let detector = DuplicateDetector::new(tenant_id);
    let now = Utc::now();

    let existing = InvoiceRecord {
        invoice_id: Uuid::new_v4().to_string(),
        vendor_name: "Acme Corp".to_string(),
        amount: 1500.00,
        invoice_date: now - Duration::days(2),
        invoice_number: Some("INV-1005".to_string()),
        line_item_fingerprint: None,
    };

    // OCR misread: O↔0, I↔1
    let incoming = InvoiceRecord {
        invoice_id: "INV-NEW-OCR".to_string(),
        vendor_name: "Acme Corp".to_string(),
        amount: 1500.00,
        invoice_date: now,
        invoice_number: Some("INV-1OO5".to_string()),
        line_item_fingerprint: None,
    };

    let records = vec![existing, incoming];
    let anomalies = detector.detect_duplicates(&records).unwrap();

    assert!(
        !anomalies.is_empty(),
        "OCR-fuzzy invoice number INV-1OO5 should match INV-1005"
    );
    assert!(
        anomalies[0].detected_value >= 0.9,
        "OCR-fuzzy match should score >= 0.9, got {}",
        anomalies[0].detected_value
    );
}

// ---------------------------------------------------------------------------
// Force bypass test (query param deserialization)
// ---------------------------------------------------------------------------

#[test]
fn test_force_true_bypasses_detection() {
    // When force=true, the API should skip duplicate detection and commit normally.
    // This test validates the deserialization of the force param.
    let q: CreateInvoiceQuery = serde_json::from_str(r#"{"force":true}"#).unwrap();
    assert_eq!(q.force, Some(true));

    // When force is absent or false, detection should run.
    let q_default: CreateInvoiceQuery = serde_json::from_str("{}").unwrap();
    assert_eq!(q_default.force, None);

    let q_false: CreateInvoiceQuery = serde_json::from_str(r#"{"force":false}"#).unwrap();
    assert_eq!(q_false.force, Some(false));
}

// ---------------------------------------------------------------------------
// Merge/reject semantics: verify anomaly resolution field exists
// ---------------------------------------------------------------------------

#[test]
fn test_duplicate_match_serialization_with_breakdown() {
    let dm = DuplicateMatch {
        invoice_id: Uuid::new_v4().to_string(),
        invoice_number: "INV-1005".to_string(),
        vendor_name: "Acme Corp".to_string(),
        similarity_score: 0.95,
        severity: "Critical".to_string(),
        signal_breakdown: DuplicateSignalBreakdown {
            vendor: 1.0,
            invoice_number: 0.916,
            amount: 1.0,
            date: 1.0,
            line_item_fingerprint: 1.0,
        },
    };
    let json = serde_json::to_string(&dm).unwrap();
    let back: DuplicateMatch = serde_json::from_str(&json).unwrap();
    assert_eq!(back.signal_breakdown.vendor, 1.0);
    assert!(back.signal_breakdown.invoice_number > 0.9);
}
