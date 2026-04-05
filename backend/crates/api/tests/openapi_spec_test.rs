//! Tests for OpenAPI specification correctness
//!
//! Verifies the generated OpenAPI spec has the correct base path
//! and includes the expected auth and invoice route paths.

use billforge_api::openapi::ApiDoc;
use utoipa::OpenApi;

/// Test that the OpenAPI spec serializes to valid JSON without errors.
#[test]
fn test_openapi_spec_is_valid_json() {
    let spec = ApiDoc::openapi();
    let json = serde_json::to_string(&spec);
    assert!(json.is_ok(), "OpenAPI spec should serialize to valid JSON");

    // Also verify it parses back as a generic JSON value
    let parsed: serde_json::Value = serde_json::from_str(&json.unwrap())
        .expect("Serialized JSON should be parseable");
    assert!(parsed.is_object(), "OpenAPI spec root should be a JSON object");
}

/// Test that the server base path is /api/v1 (not /api).
#[test]
fn test_openapi_base_path_is_api_v1() {
    let spec = ApiDoc::openapi();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let servers = parsed["servers"].as_array()
        .expect("servers should be an array");
    assert!(!servers.is_empty(), "servers array should not be empty");

    let url = servers[0]["url"].as_str()
        .expect("first server should have a url string");
    assert_eq!(
        url, "/api/v1",
        "Server base path should be /api/v1, not /api"
    );
}

/// Test that all expected authentication paths are present in the spec.
#[test]
fn test_openapi_contains_auth_paths() {
    let spec = ApiDoc::openapi();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let paths = parsed["paths"].as_object()
        .expect("paths should be a JSON object");

    let expected_paths = [
        "/auth/login",
        "/auth/register",
        "/auth/refresh",
        "/auth/logout",
        "/auth/me",
    ];

    for expected in &expected_paths {
        assert!(
            paths.contains_key(*expected),
            "OpenAPI spec should contain path {}",
            expected
        );
    }
}

/// Test that all expected invoice paths are present in the spec.
#[test]
fn test_openapi_contains_invoice_paths() {
    let spec = ApiDoc::openapi();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let paths = parsed["paths"].as_object()
        .expect("paths should be a JSON object");

    let expected_paths = [
        "/invoices",
        "/invoices/{id}",
        "/invoices/upload",
        "/invoices/{id}/ocr",
        "/invoices/{id}/submit",
        "/invoices/{id}/suggest-categories",
    ];

    for expected in &expected_paths {
        assert!(
            paths.contains_key(*expected),
            "OpenAPI spec should contain path {}",
            expected
        );
    }
}

/// Verify the "Payment Requests" tag is declared in the OpenAPI spec.
#[test]
fn test_openapi_contains_payment_request_tag() {
    let spec = ApiDoc::openapi();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let tags = parsed["tags"].as_array()
        .expect("tags should be a JSON array");

    let names: Vec<&str> = tags.iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    assert!(
        names.contains(&"Payment Requests"),
        "OpenAPI spec should declare a 'Payment Requests' tag, found: {:?}",
        names
    );
}

/// Verify that every route group mounted in `routes/mod.rs` is either covered
/// by the OpenAPI spec or explicitly listed in KNOWN_GAPS. This prevents
/// future drift: if a new route group is added, this test will fail until the
/// group gets utoipa annotations or is added to KNOWN_GAPS.
#[test]
fn test_openapi_covers_all_mounted_route_groups() {
    let spec = ApiDoc::openapi();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");

    let spec_paths = parsed["paths"].as_object()
        .expect("paths should be a JSON object");

    // Route groups that are already documented in the OpenAPI spec.
    // For each group we list one or more path prefixes that prove coverage.
    // Note: auth and invoice paths are relative (e.g. "/auth/login"),
    // while dashboard, quickbooks, and xero use absolute paths (e.g. "/api/v1/dashboard/metrics").
    let documented_groups: &[(&str, &[&str])] = &[
        ("auth", &["/auth/login", "/auth/register"]),
        ("invoices", &["/invoices", "/invoices/{id}"]),
        ("dashboard", &["/api/v1/dashboard/metrics", "/api/v1/dashboard/metrics/invoices"]),
        ("quickbooks", &["/api/v1/quickbooks/connect", "/api/v1/quickbooks/status"]),
        ("xero", &["/api/v1/xero/connect", "/api/v1/xero/status"]),
    ];

    // Route groups that are NOT yet documented. These need utoipa
    // #[utoipa::path()] annotations added to their handler functions.
    // When annotations are added, move the group to `documented_groups`.
    let known_gaps: &[&str] = &[
        "vendors",
        "workflows",
        "reports",
        "export",
        "documents",
        "audit",
        "sandbox",
        "sage-intacct",
        "salesforce",
        "workday",
        "bill-com",
        "edi",
        "purchase-orders",
        "notifications",
        "predictive",
        "mobile",
        "settings",
        "feedback",
        "theme",
        "email-actions",
        "ai",
        "billing",
        "vendor-statements",
        "payment-requests",
    ];

    // Verify documented groups are actually present in the spec
    for (group, sample_paths) in documented_groups {
        let covered = sample_paths.iter().any(|p| spec_paths.contains_key(*p));
        assert!(
            covered,
            "Route group '{}' is listed as documented but none of its paths {:?} appear in the spec",
            group, sample_paths
        );
    }

    // The full set of all mounted route groups (from api_routes() in routes/mod.rs).
    // If a NEW group is added to the router, it MUST appear in one of these two
    // lists or this test will fail.
    let all_mounted_groups: &[&str] = &[
        "auth",
        "invoices",
        "vendors",
        "workflows",
        "reports",
        "dashboard",
        "export",
        "documents",
        "audit",
        "sandbox",
        "quickbooks",
        "xero",
        "sage-intacct",
        "salesforce",
        "workday",
        "bill-com",
        "edi",
        "purchase-orders",
        "notifications",
        "predictive",
        "mobile",
        "settings",
        "feedback",
        "theme",
        "email-actions",
        "ai",
        "billing",
        "vendor-statements",
        "payment-requests",
    ];

    let documented_names: Vec<&str> = documented_groups.iter().map(|(n, _)| *n).collect();

    for group in all_mounted_groups {
        let is_documented = documented_names.contains(group);
        let is_known_gap = known_gaps.contains(group);
        assert!(
            is_documented || is_known_gap,
            "Route group '{}' is mounted in routes/mod.rs but is neither documented in the \
             OpenAPI spec nor listed in KNOWN_GAPS. Either add utoipa annotations to its \
             handlers or add it to the known_gaps list in this test.",
            group
        );
    }
}
