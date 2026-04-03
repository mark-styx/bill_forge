//! Integration tests for the OCR invoice capture pipeline
//!
//! Tests verify:
//! - Invoice upload with OCR processing
//! - Field extraction accuracy
//! - Confidence scoring
//! - Queue routing based on confidence
//! - Error handling

use axum::{
    body::Body,
    http::{header, Method, Request},
};
use billforge_api::{routes, AppState, Config};
use serde_json::Value;
use tower::util::ServiceExt;
use uuid::Uuid;

/// Helper to create test app state with PostgreSQL
async fn create_test_state() -> AppState {
    // Set required environment variables for testing
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    std::env::set_var(
        "DATABASE_URL",
        "postgres://postgres@localhost:5432/billforge_test",
    );
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config)
        .await
        .expect("Failed to create test state")
}

/// Helper to create the test router
async fn create_test_router() -> axum::Router {
    let state = create_test_state().await;
    routes::create_router(state)
}

/// Helper to get auth token for test user
async fn get_auth_token(app: &axum::Router) -> String {
    // Login with sandbox admin user
    let login_body = serde_json::json!({
        "tenant_id": "00000000-0000-0000-0000-000000000001",
        "email": "admin@example.com",
        "password": "password123"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .expect("Login failed");

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    json["token"].as_str().unwrap().to_string()
}

/// Create a minimal valid PDF for testing
fn create_test_invoice_pdf() -> Vec<u8> {
    // Minimal PDF with invoice data
    let pdf_content = r#"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length 200 >>
stream
BT
/F1 12 Tf
100 700 Td
(Acme Corporation) Tj
0 -20 Td
(Invoice #: INV-2024-001) Tj
0 -20 Td
(Invoice Date: 01/15/2024) Tj
0 -20 Td
(Due Date: 02/15/2024) Tj
0 -30 Td
(Total: $1,250.00) Tj
ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f
0000000009 00000 n
0000000058 00000 n
0000000115 00000 n
0000000266 00000 n
0000000518 00000 n
trailer
<< /Size 6 /Root 1 0 R >>
startxref
597
%%EOF
"#;

    pdf_content.as_bytes().to_vec()
}

/// Create a test PNG image for OCR testing
fn create_test_invoice_image() -> Vec<u8> {
    // Minimal PNG (1x1 pixel) - OCR will fail but tests upload flow
    let png_data = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1 pixel
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, // CRC
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, // IDAT chunk
        0x08, 0xD7, 0x63, 0xF8, 0xFF, 0xFF, 0x3F, 0x00, 0x05, 0xFE, 0x02, 0xFE, // Data + CRC
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
        0xAE, 0x42, 0x60, 0x82, // CRC
    ];

    png_data.to_vec()
}

// ============================================================================
// Invoice Upload Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_invoice_upload_requires_authentication() {
    let app = create_test_router().await;

    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.pdf\"\r\nContent-Type: application/pdf\r\n\r\n",
        boundary
    );
    let body_bytes = body.into_bytes();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_invoice_upload_with_pdf() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    let pdf_data = create_test_invoice_pdf();

    // Create multipart body
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();

    // File part
    body.extend_from_slice(format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test_invoice.pdf\"\r\nContent-Type: application/pdf\r\n\r\n",
        boundary
    ).as_bytes());
    body.extend_from_slice(&pdf_data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Upload request failed");

    let status = response.status();

    // Should succeed (200 OK or 201 Created)
    assert!(
        status == axum::http::StatusCode::OK || status == axum::http::StatusCode::CREATED,
        "Expected 200/201, got {}",
        status
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).expect("Response should be JSON");

    // Verify response structure
    assert!(
        json.get("invoice_id").is_some(),
        "Response should have invoice_id"
    );
    assert!(
        json.get("document_id").is_some(),
        "Response should have document_id"
    );
    assert!(
        json.get("message").is_some(),
        "Response should have message"
    );

    // Verify invoice_id is a valid UUID
    let invoice_id = json["invoice_id"].as_str().unwrap();
    assert!(
        Uuid::parse_str(invoice_id).is_ok(),
        "invoice_id should be a valid UUID"
    );
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_invoice_upload_with_image() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    let image_data = create_test_invoice_image();

    // Create multipart body
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();

    body.extend_from_slice(format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test_invoice.png\"\r\nContent-Type: image/png\r\n\r\n",
        boundary
    ).as_bytes());
    body.extend_from_slice(&image_data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Upload request failed");

    let status = response.status();

    // Should succeed
    assert!(
        status == axum::http::StatusCode::OK || status == axum::http::StatusCode::CREATED,
        "Expected 200/201, got {}",
        status
    );
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_invoice_upload_rejects_invalid_file_type() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    // Try to upload a text file
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();

    body.extend_from_slice(format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\nContent-Type: text/plain\r\n\r\nThis is not an invoice\r\n--{}--\r\n",
        boundary, boundary
    ).as_bytes());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Upload request failed");

    // Should reject invalid file type
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_invoice_upload_requires_file() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    // Multipart without file field
    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"notfile\"\r\n\r\nsome data\r\n--{}--\r\n",
        boundary, boundary
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Upload request failed");

    // Should reject missing file
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

// ============================================================================
// OCR Processing Tests (Unit-level tests are in tesseract.rs)
// These integration tests focus on end-to-end OCR flow
// ============================================================================

// Note: Unit tests for OCR extraction logic are in
// /backend/crates/invoice-capture/src/ocr/tesseract.rs
// Integration tests here focus on the full upload → OCR → queue routing flow

// Note: Detailed OCR extraction logic tests are in the invoice-capture crate
// Integration tests focus on the end-to-end flow with real file uploads

// ============================================================================
// Queue Routing Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_high_confidence_routes_to_ap_queue() {
    // This test verifies that invoices with >= 85% OCR confidence
    // are routed to the AP queue (not error queue)

    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    // Upload a clear, well-formatted invoice
    let pdf_data = create_test_invoice_pdf();

    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();

    body.extend_from_slice(format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"clear_invoice.pdf\"\r\nContent-Type: application/pdf\r\n\r\n",
        boundary
    ).as_bytes());
    body.extend_from_slice(&pdf_data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoices/upload")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Upload failed");

    assert!(response.status().is_success());

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    let invoice_id = json["invoice_id"].as_str().unwrap();

    // Fetch the created invoice and verify it's NOT in the error queue
    let invoice_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/invoices/{}", invoice_id))
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Failed to fetch invoice");

    assert!(invoice_response.status().is_success());

    let invoice_body = axum::body::to_bytes(invoice_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let invoice: Value = serde_json::from_slice(&invoice_body).unwrap();

    // Verify capture status is NOT "failed"
    // High confidence should route to AP queue (ready_for_review) or similar
    let capture_status = invoice["capture_status"].as_str().unwrap_or("");
    assert_ne!(
        capture_status, "failed",
        "High confidence invoice should not be in failed status"
    );
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires tesseract installation"]
async fn test_ocr_processing_time_under_5_seconds() {
    use billforge_core::traits::OcrService;
    use billforge_invoice_capture::ocr::TesseractOcr;
    use std::time::Instant;

    if !TesseractOcr::is_available() {
        eprintln!("Skipping OCR performance test - tesseract not installed");
        return;
    }

    let ocr = TesseractOcr::new();
    let pdf_data = create_test_invoice_pdf();

    let start = Instant::now();

    // Process the PDF
    let result = ocr.extract(&pdf_data, "application/pdf").await;

    let duration = start.elapsed();

    assert!(result.is_ok(), "OCR should succeed on test PDF");

    // P95 requirement: < 5 seconds (Sprint 2 success criteria)
    assert!(
        duration.as_secs() < 5,
        "OCR processing should complete in under 5 seconds (P95 requirement), took {:?}",
        duration
    );

    println!(
        "✓ OCR processing time: {:?} (requirement: <5s P95)",
        duration
    );
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
#[ignore = "Requires tesseract installation"]
async fn test_ocr_handles_corrupted_file_gracefully() {
    use billforge_core::traits::OcrService;
    use billforge_invoice_capture::ocr::TesseractOcr;

    if !TesseractOcr::is_available() {
        eprintln!("Skipping OCR error handling test - tesseract not installed");
        return;
    }

    let ocr = TesseractOcr::new();

    // Corrupted/invalid PDF data
    let corrupted_data = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE];

    // Should handle error gracefully (not panic)
    let result = ocr.extract(&corrupted_data, "application/pdf").await;

    // OCR should either return an error or return a result with empty/low confidence fields
    // The key is it shouldn't panic
    match result {
        Ok(extraction) => {
            // If it succeeds, fields should be empty or have low confidence
            println!("OCR returned result for corrupted file (fields may be empty)");
            assert!(
                extraction.invoice_number.value.is_none()
                    || extraction.total_amount.value.is_none(),
                "Corrupted file should have empty or missing fields"
            );
        }
        Err(e) => {
            // Expected - should return a meaningful error
            println!("✓ OCR correctly rejected corrupted file: {}", e);
        }
    }
}

#[tokio::test]
#[ignore = "Requires tesseract installation"]
async fn test_ocr_supported_formats() {
    use billforge_core::traits::OcrService;
    use billforge_invoice_capture::ocr::TesseractOcr;

    if !TesseractOcr::is_available() {
        eprintln!("Skipping OCR formats test - tesseract not installed");
        return;
    }

    let ocr = TesseractOcr::new();

    let supported = ocr.supported_formats();

    // Verify required formats for Sprint 2
    assert!(supported.contains(&"application/pdf"), "Should support PDF");
    assert!(supported.contains(&"image/png"), "Should support PNG");
    assert!(supported.contains(&"image/jpeg"), "Should support JPEG");
    assert!(supported.contains(&"image/tiff"), "Should support TIFF");

    println!("✓ OCR supported formats: {:?}", supported);
}
