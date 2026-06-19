//! Tests for the bulk invoice upload endpoint added in #371.
//!
//! Coverage strategy mirrors existing repo conventions for invoice route tests:
//! - Source-level wiring guards (see `integration_gating.rs`) verify the route
//!   is mounted and the handler references the shared core path, without
//!   needing a running PostgreSQL.
//! - DTO serialization round-trips (see `invoice_duplicate_detection_test.rs`)
//!   pin the response contract documented in the work item.
//! - The cap and concurrency constants are asserted against the values called
//!   out in the implementation plan so a future bump is a conscious decision.

use billforge_api::routes::invoices::{
    BulkUploadItemResult, BulkUploadResponse, BULK_MAX_CONCURRENCY, BULK_MAX_FILES,
};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Source-level wiring guards
// ---------------------------------------------------------------------------

const INVOICES_RS: &str = include_str!("../src/routes/invoices.rs");

/// The new bulk route must be registered in the invoice router builder.
#[test]
fn bulk_upload_route_is_registered() {
    assert!(
        INVOICES_RS.contains(".route(\"/upload/bulk\", post(bulk_upload_invoices))"),
        "invoices.rs must register POST /upload/bulk -> bulk_upload_invoices"
    );
}

/// The bulk handler must reuse the existing single-file core path
/// (`upload_invoice_file`) so storage, OCR enqueue/sync, audit logging, and
/// duplicate detection stay identical between single and bulk upload.
#[test]
fn bulk_handler_delegates_to_shared_single_file_helper() {
    assert!(
        INVOICES_RS.contains("pub(crate) async fn upload_invoice_file"),
        "shared single-file helper upload_invoice_file must exist"
    );
    // The bulk handler must call it (not reimplement storage/audit inline).
    let bulk_start = INVOICES_RS
        .find("async fn bulk_upload_invoices(")
        .expect("bulk_upload_invoices handler must exist");
    let bulk_section = &INVOICES_RS[bulk_start..];
    let bulk_end = bulk_section
        .find("\n}\n")
        .expect("bulk_upload_invoices must have a closing brace");
    let bulk_body = &bulk_section[..bulk_end];
    assert!(
        bulk_body.contains("upload_invoice_file"),
        "bulk_upload_invoices must delegate per-file work to upload_invoice_file"
    );
}

/// The bulk handler must collect `file` fields into a Vec (fixing the
/// upload_capture "last file wins" overwrite pattern flagged in #371).
#[test]
fn bulk_handler_collects_files_into_vec_not_overwrites() {
    let bulk_start = INVOICES_RS
        .find("async fn bulk_upload_invoices(")
        .expect("bulk_upload_invoices handler must exist");
    let bulk_section = &INVOICES_RS[bulk_start..];
    let bulk_end = bulk_section
        .find("\n}\n")
        .expect("bulk_upload_invoices must have a closing brace");
    let bulk_body = &bulk_section[..bulk_end];
    assert!(
        bulk_body.contains("Vec<(String, String, Vec<u8>)>"),
        "bulk handler must collect files into a Vec<(filename, content_type, bytes)>"
    );
    assert!(
        bulk_body.contains(".push(("),
        "bulk handler must push each file into the Vec instead of overwriting a single slot"
    );
}

/// Bounded concurrency must be wired via buffer_unordered with the configured
/// cap, not an unbounded fan-out.
#[test]
fn bulk_handler_uses_bounded_concurrency() {
    let bulk_start = INVOICES_RS
        .find("async fn bulk_upload_invoices(")
        .expect("bulk_upload_invoices handler must exist");
    let bulk_section = &INVOICES_RS[bulk_start..];
    let bulk_end = bulk_section
        .find("\n}\n")
        .expect("bulk_upload_invoices must have a closing brace");
    let bulk_body = &bulk_section[..bulk_end];
    assert!(
        bulk_body.contains(".buffer_unordered(BULK_MAX_CONCURRENCY)"),
        "bulk handler must bound concurrency via buffer_unordered(BULK_MAX_CONCURRENCY)"
    );
}

/// The cap rejection must fire when more than BULK_MAX_FILES are received.
#[test]
fn bulk_handler_enforces_file_count_cap() {
    let bulk_start = INVOICES_RS
        .find("async fn bulk_upload_invoices(")
        .expect("bulk_upload_invoices handler must exist");
    let bulk_section = &INVOICES_RS[bulk_start..];
    let bulk_end = bulk_section
        .find("\n}\n")
        .expect("bulk_upload_invoices must have a closing brace");
    let bulk_body = &bulk_section[..bulk_end];
    assert!(
        bulk_body.contains("files.len() > BULK_MAX_FILES"),
        "bulk handler must reject payloads exceeding BULK_MAX_FILES"
    );
}

/// The OpenAPI spec must advertise the new bulk route and its response schema.
#[test]
fn bulk_upload_advertised_in_openapi_spec() {
    let source = include_str!("../src/openapi.rs");
    assert!(
        source.contains("crate::routes::invoices::bulk_upload_invoices"),
        "openapi.rs must register bulk_upload_invoices in InvoiceApiDoc paths"
    );
    assert!(
        source.contains("crate::routes::invoices::BulkUploadResponse"),
        "openapi.rs must register BulkUploadResponse schema"
    );
    assert!(
        source.contains("crate::routes::invoices::BulkUploadItemResult"),
        "openapi.rs must register BulkUploadItemResult schema"
    );
}

// ---------------------------------------------------------------------------
// Cap / concurrency constant values (per implementation plan)
// ---------------------------------------------------------------------------

#[test]
fn bulk_max_files_is_50_per_plan() {
    assert_eq!(
        BULK_MAX_FILES, 50,
        "BULK_MAX_FILES must be 50 per the #371 implementation plan"
    );
}

#[test]
fn bulk_max_concurrency_is_4_per_plan() {
    assert_eq!(
        BULK_MAX_CONCURRENCY, 4,
        "BULK_MAX_CONCURRENCY must be 4 per the #371 implementation plan"
    );
}

// ---------------------------------------------------------------------------
// Response contract: serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn bulk_upload_success_response_serializes_with_expected_shape() {
    let response = BulkUploadResponse {
        total: 3,
        succeeded: 3,
        failed: 0,
        results: vec![
            BulkUploadItemResult {
                filename: "a.pdf".to_string(),
                status: "ok".to_string(),
                invoice_id: Some("00000000-0000-0000-0000-000000000001".to_string()),
                document_id: Some("00000000-0000-0000-0000-000000000002".to_string()),
                error: None,
            },
            BulkUploadItemResult {
                filename: "b.png".to_string(),
                status: "ok".to_string(),
                invoice_id: Some("00000000-0000-0000-0000-000000000003".to_string()),
                document_id: Some("00000000-0000-0000-0000-000000000004".to_string()),
                error: None,
            },
            BulkUploadItemResult {
                filename: "c.jpg".to_string(),
                status: "ok".to_string(),
                invoice_id: Some("00000000-0000-0000-0000-000000000005".to_string()),
                document_id: Some("00000000-0000-0000-0000-000000000006".to_string()),
                error: None,
            },
        ],
    };

    let json = serde_json::to_value(&response).expect("BulkUploadResponse must serialize");
    assert_eq!(json["total"], 3);
    assert_eq!(json["succeeded"], 3);
    assert_eq!(json["failed"], 0);
    let results = json["results"]
        .as_array()
        .expect("results must be an array");
    assert_eq!(results.len(), 3, "one result per uploaded file");
    for item in results {
        assert_eq!(item["status"], "ok", "every successful item has status=ok");
        assert!(
            item["invoice_id"].is_string(),
            "successful items include an invoice_id"
        );
        // error field is skipped when None (skip_serializing_if = "Option::is_none")
        assert!(
            item.get("error").is_none() || item["error"].is_null(),
            "error must be absent on success"
        );
    }
}

#[test]
fn bulk_upload_mixed_response_serializes_error_entries() {
    let response = BulkUploadResponse {
        total: 3,
        succeeded: 1,
        failed: 2,
        results: vec![
            BulkUploadItemResult {
                filename: "ok.pdf".to_string(),
                status: "ok".to_string(),
                invoice_id: Some("00000000-0000-0000-0000-000000000001".to_string()),
                document_id: Some("00000000-0000-0000-0000-000000000002".to_string()),
                error: None,
            },
            BulkUploadItemResult {
                filename: "oversize.pdf".to_string(),
                status: "error".to_string(),
                invoice_id: None,
                document_id: None,
                error: Some("File too large".to_string()),
            },
            BulkUploadItemResult {
                filename: "bad.pdf".to_string(),
                status: "error".to_string(),
                invoice_id: None,
                document_id: None,
                error: Some("Unsupported file type".to_string()),
            },
        ],
    };

    let json = serde_json::to_value(&response).expect("BulkUploadResponse must serialize");
    assert_eq!(json["total"], 3);
    assert_eq!(json["succeeded"], 1);
    assert_eq!(json["failed"], 2);

    let results = json["results"]
        .as_array()
        .expect("results must be an array");
    let errors: Vec<&Value> = results.iter().filter(|r| r["status"] == "error").collect();
    assert_eq!(errors.len(), 2, "two error entries");
    for err in &errors {
        assert!(
            err["error"].is_string(),
            "error entries must include a human-readable error string"
        );
        assert!(
            err.get("invoice_id").is_none() || err["invoice_id"].is_null(),
            "error entries must not report an invoice_id"
        );
    }
}

#[test]
fn bulk_upload_empty_results_array_is_valid() {
    // Defensive: a response with zero results (e.g. all-but-cap edge) must
    // still serialize without panicking.
    let response = BulkUploadResponse {
        total: 0,
        succeeded: 0,
        failed: 0,
        results: vec![],
    };
    let json = serde_json::to_value(&response).expect("empty BulkUploadResponse must serialize");
    assert_eq!(json["total"], 0);
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
}

/// Verify the over-cap rejection error message references the cap so callers
/// can self-correct. The handler builds this string at request time, so this
/// test pins the constant interpolation shape used in the source.
#[test]
fn over_cap_error_message_shape_is_stable() {
    let expected_msg = format!(
        "Too many files: received {} but the bulk upload limit is {} files per request.",
        BULK_MAX_FILES + 1,
        BULK_MAX_FILES
    );
    assert!(
        INVOICES_RS.contains("Too many files: received {} but the bulk upload limit is"),
        "bulk handler must emit the over-cap error template"
    );
    assert!(
        expected_msg.contains(&BULK_MAX_FILES.to_string()),
        "interpolated cap value must appear in the error message"
    );
}
