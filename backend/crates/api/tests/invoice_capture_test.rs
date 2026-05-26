//! Integration tests for the Invoice Capture endpoint.
//!
//! Tests:
//! - `test_upload_image_returns_line_items_with_confidence` - happy path
//! - `test_upload_rejects_wrong_mime_and_oversize` - validation edge cases

use axum::{
    body::Body,
    http::{header, Method, Request},
};
use billforge_api::{routes, AppState, Config};
use serde_json::Value;
use tower::util::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn create_test_state() -> AppState {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    std::env::set_var(
        "DATABASE_URL",
        "postgres://postgres@localhost:5432/billforge_test",
    );
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants_ic");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files_ic");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config)
        .await
        .expect("Failed to create test state")
}

async fn create_test_router() -> axum::Router {
    let state = create_test_state().await;
    routes::create_router(state)
}

async fn get_auth_token(app: &axum::Router) -> String {
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

/// Build a minimal multipart body containing a single `file` field with the given bytes and MIME.
fn build_multipart_body(boundary: &str, filename: &str, mime: &str, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {}\r\n\r\n", mime).as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    body
}

/// Create a tiny valid PNG (1x1 white pixel).
fn tiny_png() -> Vec<u8> {
    // Minimal PNG: 8-byte signature + IHDR + IDAT + IEND
    let signature: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

    // IHDR chunk: 1x1, 8-bit RGB
    let ihdr_data: [u8; 13] = [
        0, 0, 0, 1, // width
        0, 0, 0, 1, // height
        8, // bit depth
        2, // color type (RGB)
        0, // compression
        0, // filter
        0, // interlace
    ];
    let ihdr_crc = crc32(&[
        b"IHDR", // chunk type
        &ihdr_data[..],
    ]);
    let mut ihdr_chunk = Vec::new();
    ihdr_chunk.extend_from_slice(&0u32.to_be_bytes()); // length = 13
    ihdr_chunk.extend_from_slice(b"IHDR");
    ihdr_chunk.extend_from_slice(&ihdr_data);
    ihdr_chunk.extend_from_slice(&ihdr_crc.to_be_bytes());

    // IDAT chunk: single row with filter byte 0 + white pixel (255, 255, 255), deflate-wrapped
    let raw_data = [0u8, 255, 255, 255]; // filter=none + RGB
    let deflated = deflate_minimal(&raw_data);
    let idat_crc = crc32(&[b"IDAT", &deflated]);
    let mut idat_chunk = Vec::new();
    idat_chunk.extend_from_slice(&(deflated.len() as u32).to_be_bytes());
    idat_chunk.extend_from_slice(b"IDAT");
    idat_chunk.extend_from_slice(&deflated);
    idat_chunk.extend_from_slice(&idat_crc.to_be_bytes());

    // IEND chunk
    let iend_crc = crc32(&[b"IEND"]);
    let mut iend_chunk = Vec::new();
    iend_chunk.extend_from_slice(&0u32.to_be_bytes());
    iend_chunk.extend_from_slice(b"IEND");
    iend_chunk.extend_from_slice(&iend_crc.to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&signature);
    png.extend_from_slice(&ihdr_chunk);
    png.extend_from_slice(&idat_chunk);
    png.extend_from_slice(&iend_chunk);
    png
}

/// Minimal deflate for very small payloads (stored block, no compression).
fn deflate_minimal(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // zlib header
    out.push(0x78); // CMF
    out.push(0x01); // FLG (no dict, level 0)
                    // Stored block
    out.push(0x01); // BFINAL=1, BTYPE=00 (stored)
    let len = data.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    // Adler-32 checksum
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// CRC-32 as used by PNG (ISO 3309 / ITU-T V.42).
fn crc32(data: &[&[u8]]) -> u32 {
    // CRC lookup table
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                if c & 1 == 1 {
                    c = 0xEDB88320 ^ (c >> 1);
                } else {
                    c >>= 1;
                }
            }
            t[n as usize] = c;
        }
        t
    });

    let mut crc = 0xFFFFFFFF_u32;
    for slice in data {
        for &byte in *slice {
            let idx = ((crc ^ byte as u32) & 0xFF) as usize;
            crc = table[idx] ^ (crc >> 8);
        }
    }
    !crc
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_upload_image_returns_line_items_with_confidence() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    let png_data = tiny_png();
    let boundary = "----TestBoundary12345";
    let body = build_multipart_body(boundary, "invoice.png", "image/png", &png_data);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoice-captures")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    let status = response.status();
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body_bytes).unwrap();

    // We expect 201 Created (OCR ran, even though the 1x1 pixel won't extract meaningful lines)
    assert!(
        status == axum::http::StatusCode::CREATED,
        "Expected 201, got {}: {:?}",
        status,
        json
    );

    // Verify response structure
    assert!(
        json["capture_id"].is_string(),
        "capture_id should be a string"
    );
    assert!(json["provider"].is_string(), "provider should be a string");
    assert!(
        json["overall_confidence"].is_number(),
        "overall_confidence should be a number"
    );

    // Confidence must be between 0 and 1
    let conf = json["overall_confidence"].as_f64().unwrap();
    assert!(
        (0.0..=1.0).contains(&conf),
        "Confidence {} not in [0, 1]",
        conf
    );

    // line_items is an array (may be empty for a 1x1 pixel)
    assert!(
        json["line_items"].is_array(),
        "line_items should be an array"
    );
}

#[tokio::test]
#[ignore = "Requires running PostgreSQL and seeded sandbox tenant"]
async fn test_upload_rejects_wrong_mime_and_oversize() {
    let app = create_test_router().await;
    let token = get_auth_token(&app).await;

    // --- Test wrong MIME type (text/plain) ---
    let boundary = "----TestBoundaryBadMime";
    let body = build_multipart_body(boundary, "notes.txt", "text/plain", b"hello world");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoice-captures")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    let status = response.status();
    assert_eq!(
        status,
        axum::http::StatusCode::BAD_REQUEST,
        "Expected 400 for wrong MIME type, got {}",
        status
    );

    // --- Test oversize (> 10 MB) ---
    let boundary2 = "----TestBoundaryOversize";
    let oversize_data = vec![0u8; 10 * 1024 * 1024 + 1]; // 10 MB + 1 byte
    let body2 = build_multipart_body(boundary2, "big.png", "image/png", &oversize_data);

    let response2 = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/invoice-captures")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={}", boundary2),
                )
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .body(Body::from(body2))
                .unwrap(),
        )
        .await
        .expect("Request failed");

    let status2 = response2.status();
    // The oversize file will be rejected with 400 (validation error from our handler)
    // since axum's default body limit may reject first, or our manual check.
    assert!(
        status2 == axum::http::StatusCode::BAD_REQUEST
            || status2 == axum::http::StatusCode::PAYLOAD_TOO_LARGE,
        "Expected 400 or 413 for oversize, got {}",
        status2
    );
}
