//! Route-existence tests for predictive analytics endpoints.
//!
//! Verifies that the Axum router in predictive.rs registers routes matching
//! the canonical path set and that the hand-written openapi.json agrees.
//! Also locks in that the buggy `/rules/{id}/update` path stays unregistered.
//!
//! Run: `cargo test -p billforge-api --test predictive_routes`

use billforge_api::openapi::openapi_doc;

/// Verify the Axum router source wires the canonical route set.
#[test]
fn test_predictive_router_registers_canonical_routes() {
    let source = include_str!("../src/routes/predictive.rs");

    // Forecasts
    assert!(
        source.contains(".route(\"/forecasts\", get(get_forecasts))"),
        "predictive.rs must register GET /forecasts"
    );
    assert!(
        source.contains(".route(\"/forecasts/generate\", post(generate_forecast))"),
        "predictive.rs must register POST /forecasts/generate"
    );

    // Alerts
    assert!(
        source.contains(".route(\"/alerts\", get(get_budget_alerts))"),
        "predictive.rs must register GET /alerts"
    );
    assert!(
        source.contains(".route(\"/alerts/:alert_id/dismiss\", post(dismiss_alert))"),
        "predictive.rs must register POST /alerts/:alert_id/dismiss"
    );

    // Rules
    assert!(
        source.contains(".route(\"/rules\", get(get_anomaly_rules))"),
        "predictive.rs must register GET /rules"
    );
    assert!(
        source.contains(".route(\"/rules\", post(configure_anomaly_rule))"),
        "predictive.rs must register POST /rules"
    );
    assert!(
        source.contains(".route(\"/rules/:rule_id\", get(get_anomaly_rule))"),
        "predictive.rs must register GET /rules/:rule_id"
    );
    assert!(
        source.contains(".route(\"/rules/:rule_id\", post(update_anomaly_rule))"),
        "predictive.rs must register POST /rules/:rule_id"
    );
}

/// Regression guard: the buggy /rules/{id}/update path must NOT be registered.
#[test]
fn test_predictive_router_does_not_register_rules_update_subpath() {
    let source = include_str!("../src/routes/predictive.rs");
    assert!(
        !source.contains("/update"),
        "predictive.rs must NOT contain a route with '/update' subpath — \
         POST /rules/:rule_id is the canonical update route"
    );
}

/// Verify the utoipa-generated OpenAPI spec contains the canonical predictive paths.
#[test]
fn test_openapi_spec_contains_canonical_predictive_paths() {
    let spec = openapi_doc();
    let json = serde_json::to_string(&spec).expect("spec serializes");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let paths = parsed["paths"]
        .as_object()
        .expect("paths should be a JSON object");

    // The utoipa annotations in predictive.rs use the full /api/v1/... prefix
    let expected_paths = [
        "/api/v1/analytics/predictive/forecasts",
        "/api/v1/analytics/predictive/forecasts/generate",
        "/api/v1/analytics/predictive/forecasts/{id}",
        "/api/v1/analytics/predictive/anomalies",
        "/api/v1/analytics/predictive/anomalies/detect",
        "/api/v1/analytics/predictive/anomalies/{id}/acknowledge",
        "/api/v1/analytics/predictive/alerts",
        "/api/v1/analytics/predictive/alerts/{alert_id}/dismiss",
        "/api/v1/analytics/predictive/rules",
        "/api/v1/analytics/predictive/rules/{rule_id}",
    ];

    for expected in &expected_paths {
        assert!(
            paths.contains_key(*expected),
            "utoipa-generated spec must contain path {}",
            expected
        );
    }
}

/// Verify the hand-written openapi.json (shared-types) has paths aligned to
/// the runtime router. This reads the committed file, not the utoipa output.
#[test]
fn test_shared_types_openapi_matches_runtime_paths() {
    let openapi_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../packages/shared-types/openapi.json"
    ))
    .expect("shared-types/openapi.json parses");
    let paths = openapi_json["paths"]
        .as_object()
        .expect("paths should be a JSON object");

    // Canonical paths that must exist (matching the Axum router)
    let must_exist = [
        "/api/v1/analytics/predictive/forecasts",
        "/api/v1/analytics/predictive/forecasts/generate",
        "/api/v1/analytics/predictive/forecasts/{id}",
        "/api/v1/analytics/predictive/alerts",
        "/api/v1/analytics/predictive/alerts/{alert_id}/dismiss",
        "/api/v1/analytics/predictive/rules",
        "/api/v1/analytics/predictive/rules/{rule_id}",
    ];

    for path in &must_exist {
        assert!(
            paths.contains_key(*path),
            "openapi.json must contain canonical path {}",
            path
        );
    }

    // Drifted paths that must NOT exist
    let must_not_exist = [
        "/api/v1/analytics/predictive/budget-alerts",
        "/api/v1/analytics/predictive/budget-alerts/{id}/dismiss",
        "/api/v1/analytics/predictive/anomaly-rules",
        "/api/v1/analytics/predictive/anomaly-rules/{id}",
    ];

    for path in &must_not_exist {
        assert!(
            !paths.contains_key(*path),
            "openapi.json must NOT contain drifted path {} (use the canonical form instead)",
            path
        );
    }
}

/// Verify the rules/{rule_id} path in openapi.json uses POST (not PUT) for
/// update_anomaly_rule, matching the Axum router which registers POST.
#[test]
fn test_openapi_rules_rule_id_uses_post_for_update() {
    let openapi_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../packages/shared-types/openapi.json"
    ))
    .expect("shared-types/openapi.json parses");

    let rule_path = &openapi_json["paths"]["/api/v1/analytics/predictive/rules/{rule_id}"];
    assert!(
        rule_path.get("post").is_some(),
        "rules/{{rule_id}} must have a POST method for update_anomaly_rule"
    );
    assert!(
        rule_path.get("put").is_none(),
        "rules/{{rule_id}} must NOT have PUT — the Axum router registers POST"
    );
}
