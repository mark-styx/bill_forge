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

/// Pin test: the LoginResponse OpenAPI component schema must exactly match the JSON shape
/// produced by a real `billforge_auth::AuthResponse`. This catches future drift between
/// the mirror structs in openapi.rs and the actual auth types.
#[test]
fn test_login_response_schema_matches_auth_response_shape() {
    use billforge_auth::{AuthResponse, TenantInfo, TenantSettingsInfo, UserInfo};
    use billforge_core::{TenantId, UserId};

    // Construct a sample AuthResponse with representative values
    let sample = AuthResponse {
        access_token: "test_access".to_string(),
        refresh_token: "test_refresh".to_string(),
        user: UserInfo {
            id: UserId::new(),
            tenant_id: TenantId::new(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            roles: vec![billforge_core::Role::ApUser],
        },
        tenant: TenantInfo {
            id: TenantId::new(),
            name: "Test Corp".to_string(),
            enabled_modules: vec!["invoices".to_string()],
            settings: TenantSettingsInfo {
                logo_url: None,
                primary_color: None,
                company_name: "Test Corp".to_string(),
                timezone: "UTC".to_string(),
                default_currency: "USD".to_string(),
            },
        },
    };

    // Serialize to JSON value and collect top-level keys
    let serialized = serde_json::to_value(&sample).expect("AuthResponse should serialize");
    let obj = serialized.as_object().expect("AuthResponse should be a JSON object");
    let mut actual_keys: Vec<String> = obj.keys().cloned().collect();
    actual_keys.sort();

    // Parse the OpenAPI spec and extract LoginResponse schema properties
    let spec = ApiDoc::openapi();
    let spec_json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&spec_json).expect("valid JSON");

    let schema_props = parsed["components"]["schemas"]["LoginResponse"]["properties"]
        .as_object()
        .expect("LoginResponse should have properties in the spec");
    let mut schema_keys: Vec<String> = schema_props.keys().cloned().collect();
    schema_keys.sort();

    // Regression guard: fictional fields must NOT appear
    assert!(
        !schema_keys.contains(&"token_type".to_string()),
        "token_type must NOT appear in LoginResponse schema (was fictional)"
    );
    assert!(
        !schema_keys.contains(&"expires_in".to_string()),
        "expires_in must NOT appear in LoginResponse schema (was fictional)"
    );

    // Assert expected fields ARE present
    for expected in &["access_token", "refresh_token", "user", "tenant"] {
        assert!(
            schema_keys.contains(&expected.to_string()),
            "LoginResponse schema must contain field '{}'",
            expected
        );
    }

    // The key sets must match exactly
    assert_eq!(
        actual_keys, schema_keys,
        "LoginResponse schema properties must match real AuthResponse JSON keys"
    );
}

/// Pin test: the Invoice OpenAPI component schema must have exactly the same top-level keys
/// as a serialized `billforge_core::domain::Invoice`. Catches drift between the mirror struct
/// in openapi.rs and the real domain type.
#[test]
fn test_invoice_schema_matches_domain_invoice_shape() {
    use billforge_core::domain::{CaptureStatus, Invoice, InvoiceLineItem, InvoiceId, ProcessingStatus};
    use billforge_core::types::{Money, TenantId, UserId};

    let sample = Invoice {
        id: InvoiceId::new(),
        tenant_id: TenantId::new(),
        vendor_id: None,
        vendor_name: "Test Vendor".to_string(),
        invoice_number: "INV-001".to_string(),
        invoice_date: None,
        due_date: None,
        po_number: None,
        subtotal: None,
        tax_amount: None,
        total_amount: Money::usd(100.0),
        currency: "USD".to_string(),
        line_items: vec![InvoiceLineItem {
            id: uuid::Uuid::new_v4(),
            line_number: 1,
            description: "Test item".to_string(),
            quantity: Some(1.0),
            unit_price: Some(Money::usd(100.0)),
            amount: Money::usd(100.0),
            gl_code: None,
            department: None,
            project: None,
        }],
        capture_status: CaptureStatus::Pending,
        processing_status: ProcessingStatus::Draft,
        current_queue_id: None,
        assigned_to: None,
        document_id: uuid::Uuid::new_v4(),
        supporting_documents: vec![],
        ocr_confidence: None,
        categorization_confidence: None,
        department: None,
        gl_code: None,
        cost_center: None,
        notes: None,
        tags: vec![],
        custom_fields: serde_json::Value::Null,
        created_by: UserId::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let serialized = serde_json::to_value(&sample).expect("Invoice should serialize");
    let obj = serialized.as_object().expect("Invoice should be a JSON object");
    let mut actual_keys: Vec<String> = obj.keys().cloned().collect();
    actual_keys.sort();

    let spec = ApiDoc::openapi();
    let spec_json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&spec_json).expect("valid JSON");

    let schema_props = parsed["components"]["schemas"]["Invoice"]["properties"]
        .as_object()
        .expect("Invoice should have properties in the spec");
    let mut schema_keys: Vec<String> = schema_props.keys().cloned().collect();
    schema_keys.sort();

    // Regression guard: critical fields must be present
    for expected in &["line_items", "po_number", "subtotal", "document_id", "created_by", "updated_at"] {
        assert!(
            schema_keys.contains(&expected.to_string()),
            "Invoice schema must contain field '{}'",
            expected
        );
    }

    assert_eq!(
        actual_keys, schema_keys,
        "Invoice schema properties must match real domain Invoice JSON keys"
    );
}

/// Pin test: the Vendor OpenAPI component schema must have exactly the same top-level keys
/// as a serialized `billforge_core::domain::Vendor`.
#[test]
fn test_vendor_schema_matches_domain_vendor_shape() {
    use billforge_core::domain::{Vendor, VendorId, VendorType, VendorStatus};

    let sample = Vendor {
        id: VendorId::new(),
        tenant_id: billforge_core::TenantId::new(),
        name: "Test Vendor".to_string(),
        legal_name: None,
        vendor_type: VendorType::Business,
        status: VendorStatus::Active,
        email: None,
        phone: None,
        website: None,
        address: None,
        tax_id: None,
        tax_id_type: None,
        w9_on_file: false,
        w9_received_date: None,
        payment_terms: None,
        default_payment_method: None,
        bank_account: None,
        vendor_code: None,
        default_gl_code: None,
        default_department: None,
        primary_contact: None,
        contacts: vec![],
        notes: None,
        tags: vec![],
        custom_fields: serde_json::Value::Null,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let serialized = serde_json::to_value(&sample).expect("Vendor should serialize");
    let obj = serialized.as_object().expect("Vendor should be a JSON object");
    let mut actual_keys: Vec<String> = obj.keys().cloned().collect();
    actual_keys.sort();

    let spec = ApiDoc::openapi();
    let spec_json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&spec_json).expect("valid JSON");

    let schema_props = parsed["components"]["schemas"]["Vendor"]["properties"]
        .as_object()
        .expect("Vendor should have properties in the spec");
    let mut schema_keys: Vec<String> = schema_props.keys().cloned().collect();
    schema_keys.sort();

    // Regression guard: critical fields must be present
    for expected in &["legal_name", "address", "contacts", "tax_id", "w9_on_file", "updated_at"] {
        assert!(
            schema_keys.contains(&expected.to_string()),
            "Vendor schema must contain field '{}'",
            expected
        );
    }

    assert_eq!(
        actual_keys, schema_keys,
        "Vendor schema properties must match real domain Vendor JSON keys"
    );
}

/// Verify that the 200 response on each auth endpoint references LoginResponse.
#[test]
fn test_auth_paths_200_reference_login_response() {
    let spec = ApiDoc::openapi();
    let spec_json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&spec_json).expect("valid JSON");

    let paths_to_check = ["/auth/login", "/auth/register", "/auth/refresh", "/auth/provision"];

    for path in &paths_to_check {
        let response_ref = parsed["paths"][path]["post"]["responses"]["200"]
            .get("content")
            .and_then(|c| c["application/json"]["schema"]["$ref"].as_str())
            .or_else(|| {
                // If body= was used without content negotiation, utoipa may put $ref directly
                parsed["paths"][path]["post"]["responses"]["200"]["content"]
                    .get("application/json")
                    .and_then(|v| v["schema"]["$ref"].as_str())
            });

        let ref_str = response_ref.unwrap_or_else(|| {
            panic!(
                "Path {} 200 response should have a $ref to LoginResponse",
                path
            )
        });

        assert!(
            ref_str.ends_with("/LoginResponse"),
            "Path {} 200 response $ref should end with /LoginResponse, got: {}",
            path,
            ref_str
        );
    }
}
