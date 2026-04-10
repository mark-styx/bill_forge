//! Feature flag tests for modular ERP integrations
//!
//! Verifies that the default feature set includes all ERP integrations
//! and that the core router builds correctly with all features enabled.
//!
//! Slim builds (no-default-features) are validated at build time via:
//!   cargo check -p billforge-api --no-default-features
//!   cargo check -p billforge-api --no-default-features --features quickbooks

/// Verify feature-gated modules are accessible with default features.
/// This test proves the #[cfg(feature = "...")] gates resolve correctly.
/// If any module is missing (feature not enabled), compilation fails.
#[test]
fn test_erp_route_modules_accessible_with_defaults() {
    let _ = billforge_api::routes::quickbooks::routes;
    let _ = billforge_api::routes::xero::routes;
    let _ = billforge_api::routes::sage_intacct::routes;
    let _ = billforge_api::routes::salesforce::routes;
    let _ = billforge_api::routes::workday::routes;
    let _ = billforge_api::routes::bill_com::routes;
    let _ = billforge_api::routes::edi::routes;
    let _ = billforge_api::routes::purchase_orders::routes;
}

/// Verify core (non-ERP) route modules are always accessible,
/// regardless of feature flag configuration.
#[test]
fn test_core_route_modules_always_accessible() {
    // These pub modules are NOT behind feature flags and must always compile.
    let _ = billforge_api::routes::auth::routes;
    let _ = billforge_api::routes::invoices::routes;
    let _ = billforge_api::routes::dashboard::routes;
    let _ = billforge_api::routes::notifications::routes;
    let _ = billforge_api::routes::billing::routes;
    let _ = billforge_api::routes::ai::routes;
    let _ = billforge_api::routes::payment_requests::routes;
    let _ = billforge_api::routes::vendor_statements::routes;
    let _ = billforge_api::routes::routing::routes;
    let _ = billforge_api::routes::mobile::routes;
    let _ = billforge_api::routes::predictive::routes;
    let _ = billforge_api::routes::theme::org_routes;
    let _ = billforge_api::routes::theme::user_routes;
    let _ = billforge_api::routes::email_actions::routes;
}
