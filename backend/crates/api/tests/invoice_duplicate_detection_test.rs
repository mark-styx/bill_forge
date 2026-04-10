//! Integration tests for duplicate detection in invoice submission (#131).
//!
//! Tests cover:
//! - DuplicateDetector correctly flags near-identical invoices
//! - DuplicateDetector returns no matches for unique invoices
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

#[derive(serde::Serialize, serde::Deserialize)]
struct DuplicateMatch {
    invoice_id: String,
    invoice_number: String,
    vendor_name: String,
    similarity_score: f64,
    severity: String,
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
    };

    // New incoming invoice: same vendor, same amount, close date
    let incoming = InvoiceRecord {
        invoice_id: "INV-NEW-001".to_string(),
        vendor_name: "Acme Corp".to_string(),
        amount: 1500.00,
        invoice_date: now - Duration::days(3),
    };

    // Unrelated invoice that should not match
    let other = InvoiceRecord {
        invoice_id: Uuid::new_v4().to_string(),
        vendor_name: "Completely Different LLC".to_string(),
        amount: 7500.00,
        invoice_date: now - Duration::days(60),
    };

    let records = vec![existing.clone(), incoming.clone(), other];
    let anomalies = detector.detect_duplicates(&records).unwrap();

    // Should detect at least one duplicate (existing <-> incoming)
    assert!(!anomalies.is_empty(), "Should detect duplicate pair");

    // Find the anomaly that references the incoming invoice
    let dup = anomalies.iter().find(|a| {
        let meta = &a.metadata;
        let id1 = meta.get("invoice1").and_then(|v| v.get("id")).and_then(|v| v.as_str());
        let id2 = meta.get("invoice2").and_then(|v| v.get("id")).and_then(|v| v.as_str());
        id1 == Some("INV-NEW-001") || id2 == Some("INV-NEW-001")
    });
    assert!(dup.is_some(), "Anomaly should reference the incoming invoice");

    let anomaly = dup.unwrap();
    assert!(
        anomaly.severity == AnomalySeverity::High || anomaly.severity == AnomalySeverity::Critical,
        "Expected High or Critical severity, got {:?}",
        anomaly.severity
    );
    assert!(anomaly.detected_value > 0.8, "Similarity should exceed 0.8 threshold");
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
    };

    // Completely different vendor, amount, and distant date
    let incoming = InvoiceRecord {
        invoice_id: "INV-UNIQUE-999".to_string(),
        vendor_name: "Zenith Supplies Inc".to_string(),
        amount: 420.50,
        invoice_date: now,
    };

    let records = vec![existing, incoming];
    let anomalies = detector.detect_duplicates(&records).unwrap();

    assert!(
        anomalies.is_empty(),
        "Should not detect duplicates for unrelated invoices, got {} anomalies",
        anomalies.len()
    );
}
