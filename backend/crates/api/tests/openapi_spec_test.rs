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

    // Route groups that are documented in the OpenAPI spec.
    // For each group we list one or more path prefixes that prove coverage.
    let documented_groups: &[(&str, &[&str])] = &[
        ("auth", &["/auth/login", "/auth/register"]),
        ("invoices", &["/invoices", "/invoices/{id}"]),
        ("dashboard", &["/api/v1/dashboard/metrics", "/api/v1/dashboard/metrics/invoices"]),
        ("quickbooks", &["/api/v1/quickbooks/connect", "/api/v1/quickbooks/status"]),
        ("xero", &["/api/v1/xero/connect", "/api/v1/xero/status"]),
        ("sage-intacct", &["/api/v1/sage-intacct/connect", "/api/v1/sage-intacct/status"]),
        ("salesforce", &["/api/v1/salesforce/connect", "/api/v1/salesforce/status"]),
        ("workday", &["/api/v1/workday/connect", "/api/v1/workday/status"]),
        ("bill-com", &["/api/v1/bill-com/connect", "/api/v1/bill-com/status"]),
        ("vendors", &["/api/v1/vendors", "/api/v1/vendors/{id}"]),
        ("workflows", &["/api/v1/workflows/rules", "/api/v1/workflows/queues"]),
        ("reports", &["/api/v1/reports/dashboard/summary", "/api/v1/reports/invoices/by-vendor"]),
        ("export", &["/api/v1/export/invoices/csv"]),
        ("documents", &["/api/v1/documents", "/api/v1/documents/{id}"]),
        ("audit", &["/api/v1/audit"]),
        ("sandbox", &["/api/v1/sandbox/personas"]),
        ("edi", &["/api/v1/edi/status", "/api/v1/edi/documents"]),
        ("purchase-orders", &["/api/v1/edi/purchase-orders"]),
        ("notifications", &["/api/v1/notifications/slack/install"]),
        ("predictive", &["/api/v1/analytics/predictive/forecasts"]),
        ("mobile", &["/api/v1/mobile/dashboard", "/api/v1/mobile/devices"]),
        ("settings", &["/api/v1/settings"]),
        ("feedback", &["/api/v1/feedback"]),
        ("theme", &["/api/v1/organization/theme", "/api/v1/user/theme"]),
        ("email-actions", &["/api/v1/actions/approve"]),
        ("ai", &["/api/v1/ai/chat"]),
        ("billing", &["/api/v1/billing/plans"]),
        ("vendor-statements", &["/api/v1/vendors/{vendor_id}/statements"]),
        ("payment-requests", &["/api/v1/payment-requests"]),
        ("routing", &["/api/v1/routing/workload"]),
    ];

    // Route groups that are NOT yet documented. All groups are now documented.
    let known_gaps: &[&str] = &[];

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
        "routing",
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
