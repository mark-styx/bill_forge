//! Integration tests verifying that `/analytics/predictive` routes are gated
//! behind the `Reporting` module via mount-site middleware (`require_reporting`).
//!
//! These tests mirror the approach in `module_gating_reports_documents.rs`:
//! - Verify `TenantContext::has_module(Module::Reporting)` gates access.
//! - Verify `Error::ModuleNotAvailable("Reporting")` maps to HTTP 402.
//! - Verify the route mount in `routes/mod.rs` applies `require_reporting`.

use billforge_core::{Error, Module, TenantContext, TenantId, TenantSettings};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Predictive analytics gating logic
// ---------------------------------------------------------------------------

#[test]
fn test_tenant_without_reporting_cannot_access_predictive() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Capture Only Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture],
        settings: TenantSettings::default(),
    };
    assert!(
        !ctx.has_module(Module::Reporting),
        "Tenant with only InvoiceCapture must not have Reporting (predictive gate)"
    );
}

#[test]
fn test_tenant_with_reporting_can_access_predictive() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Reporting Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture, Module::Reporting],
        settings: TenantSettings::default(),
    };
    assert!(
        ctx.has_module(Module::Reporting),
        "Tenant with Reporting enabled must pass the predictive gate"
    );
}

// ---------------------------------------------------------------------------
// Error contract: ModuleNotAvailable("Reporting") → 402
// ---------------------------------------------------------------------------

#[test]
fn test_module_not_available_reporting_maps_to_402() {
    let err = Error::ModuleNotAvailable("Reporting".to_string());
    assert_eq!(
        err.status_code(),
        402,
        "ModuleNotAvailable must be HTTP 402"
    );
    assert_eq!(err.error_code(), "MODULE_NOT_AVAILABLE");
}

// ---------------------------------------------------------------------------
// Route wiring guard: verify require_reporting is applied in routes/mod.rs
// ---------------------------------------------------------------------------

#[test]
fn test_predictive_nest_uses_require_reporting_middleware() {
    let source = include_str!("../src/routes/mod.rs");
    assert!(
        source.contains("require_reporting"),
        "routes/mod.rs must import require_reporting"
    );
    assert!(
        source.contains("predictive::routes().layer(middleware::from_fn(require_reporting))"),
        "routes/mod.rs must wrap predictive nest with require_reporting layer"
    );
}

// ---------------------------------------------------------------------------
// Compile-time proof that require_reporting middleware exists
// ---------------------------------------------------------------------------

#[test]
fn test_require_reporting_middleware_is_importable() {
    // Compile-time proof that the middleware function is public and importable.
    use billforge_api::middleware::require_reporting;
    // Just binding the function reference proves it exists and is accessible.
    let _fn_ref = require_reporting;
}
