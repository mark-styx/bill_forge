//! Tests for External API (PAT-authenticated public API) in the OpenAPI spec.
//!
//! Verifies that all five external endpoints appear in the generated contract
//! with the correct HTTP methods, the "External API" tag, and bearer security.

#![allow(warnings)]

use billforge_api::openapi::openapi_doc;

/// Helper: serialize the OpenAPI spec to a JSON Value.
fn spec_json() -> serde_json::Value {
    let spec = openapi_doc();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    serde_json::from_str(&json).expect("valid JSON")
}

#[test]
fn test_external_api_paths_exist() {
    let parsed = spec_json();
    let paths = parsed["paths"]
        .as_object()
        .expect("paths should be a JSON object");

    let expected = [
        "/api/external/v1/invoices",
        "/api/external/v1/invoices/{id}",
        "/api/external/v1/webhook-subscriptions",
        "/api/external/v1/webhook-subscriptions/{id}",
    ];

    for path in &expected {
        assert!(
            paths.contains_key(*path),
            "OpenAPI spec should contain external path {}",
            path
        );
    }
}

#[test]
fn test_external_api_methods() {
    let parsed = spec_json();

    // GET /api/external/v1/invoices
    let invoices = &parsed["paths"]["/api/external/v1/invoices"];
    assert!(invoices.get("get").is_some(), "invoices should have GET");

    // GET /api/external/v1/invoices/{id}
    let invoice_by_id = &parsed["paths"]["/api/external/v1/invoices/{id}"];
    assert!(
        invoice_by_id.get("get").is_some(),
        "invoices/{{id}} should have GET"
    );

    // POST + GET /api/external/v1/webhook-subscriptions
    let subs = &parsed["paths"]["/api/external/v1/webhook-subscriptions"];
    assert!(
        subs.get("post").is_some(),
        "webhook-subscriptions should have POST"
    );
    assert!(
        subs.get("get").is_some(),
        "webhook-subscriptions should have GET"
    );

    // DELETE /api/external/v1/webhook-subscriptions/{id}
    let sub_by_id = &parsed["paths"]["/api/external/v1/webhook-subscriptions/{id}"];
    assert!(
        sub_by_id.get("delete").is_some(),
        "webhook-subscriptions/{{id}} should have DELETE"
    );
}

#[test]
fn test_external_api_tag() {
    let parsed = spec_json();
    let paths = parsed["paths"].as_object().unwrap();

    // Collect all external-api paths and check that each operation carries the tag
    let external_paths: Vec<&str> = paths
        .keys()
        .filter(|k| k.starts_with("/api/external/v1/"))
        .map(|k| k.as_str())
        .collect();

    assert!(!external_paths.is_empty(), "should have external paths");

    for path_key in &external_paths {
        let path_obj = &paths[*path_key];
        for (method, op) in path_obj.as_object().unwrap() {
            if method == "parameters" {
                continue;
            }
            let tags = op["tags"]
                .as_array()
                .unwrap_or_else(|| panic!("{} {} should have tags array", method, path_key));
            let tag_strs: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
            assert!(
                tag_strs.contains(&"External API"),
                "{} {} should have 'External API' tag, got {:?}",
                method,
                path_key,
                tag_strs
            );
        }
    }
}

#[test]
fn test_external_api_bearer_security() {
    let parsed = spec_json();

    // Check that at least one external path has a security requirement
    let invoices_get = &parsed["paths"]["/api/external/v1/invoices"]["get"];
    let security = invoices_get["security"].as_array();
    assert!(
        security.is_some(),
        "GET /api/external/v1/invoices should have a security requirement"
    );

    let has_bearer = security.unwrap().iter().any(|s| {
        s.as_object()
            .map(|o| o.contains_key("bearer_auth"))
            .unwrap_or(false)
    });
    assert!(
        has_bearer,
        "GET /api/external/v1/invoices should reference bearer_auth security scheme"
    );
}

#[test]
fn test_external_api_schemas_in_components() {
    let parsed = spec_json();
    let schemas = parsed["components"]["schemas"]
        .as_object()
        .expect("components.schemas should be an object");

    let expected_schemas = [
        "CreateWebhookSubscriptionRequest",
        "WebhookSubscriptionResponse",
        "WebhookSubscriptionListResponse",
        "PublicSuccessResponse",
        "ListInvoicesQuery",
    ];

    for name in &expected_schemas {
        assert!(
            schemas.contains_key(*name),
            "components.schemas should contain {}",
            name
        );
    }
}
