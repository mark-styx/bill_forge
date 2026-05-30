//! Integration tests verifying that Dashboard, Notifications, and Routing
//! routes are gated by the `InvoiceProcessingAccess` extractor.
//!
//! These tests mirror the approach in `ai_billing_routes_test.rs`:
//! - Verify the extractor import exists in the route source (compile-time wiring).
//! - Verify `TenantContext::has_module(Module::InvoiceProcessing)` gates access.
//! - Verify `Error::ModuleNotAvailable` maps to HTTP 402.

use billforge_core::{Module, TenantContext, TenantId, TenantSettings};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TenantContext gating logic
// ---------------------------------------------------------------------------

#[test]
fn test_tenant_without_invoice_processing_cannot_access_dashboard_notifications_routing() {
    // Tenant with only InvoiceCapture — should NOT pass the InvoiceProcessingAccess check.
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Capture Only Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture],
        settings: TenantSettings::default(),
    };
    assert!(
        !ctx.has_module(Module::InvoiceProcessing),
        "Tenant with only InvoiceCapture must not have InvoiceProcessing"
    );
}

#[test]
fn test_tenant_with_invoice_processing_can_access_dashboard_notifications_routing() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Full Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture, Module::InvoiceProcessing],
        settings: TenantSettings::default(),
    };
    assert!(
        ctx.has_module(Module::InvoiceProcessing),
        "Tenant with InvoiceProcessing enabled must pass the check"
    );
}

// ---------------------------------------------------------------------------
// Error contract: ModuleNotAvailable("Invoice Processing") → 402
// ---------------------------------------------------------------------------

#[test]
fn test_module_not_available_invoice_processing_maps_to_402() {
    use billforge_core::Error;
    let err = Error::ModuleNotAvailable("Invoice Processing".to_string());
    assert_eq!(
        err.status_code(),
        402,
        "ModuleNotAvailable must be HTTP 402"
    );
    assert_eq!(err.error_code(), "MODULE_NOT_AVAILABLE");
}

// ---------------------------------------------------------------------------
// Route wiring guards: verify extractor import in source files
// ---------------------------------------------------------------------------

#[test]
fn test_dashboard_handlers_use_invoice_processing_access() {
    let source = include_str!("../src/routes/dashboard.rs");
    assert!(
        source.contains("InvoiceProcessingAccess"),
        "dashboard.rs must import InvoiceProcessingAccess"
    );
    // Every handler should use the extractor, not bare TenantCtx
    assert!(
        !source.contains("TenantCtx(tenant): TenantCtx"),
        "dashboard.rs must not use bare TenantCtx — use InvoiceProcessingAccess instead"
    );
}

#[test]
fn test_notifications_handlers_use_invoice_processing_access() {
    let source = include_str!("../src/routes/notifications.rs");
    assert!(
        source.contains("InvoiceProcessingAccess"),
        "notifications.rs must import InvoiceProcessingAccess"
    );
    assert!(
        !source.contains("auth_user: AuthUser"),
        "notifications.rs must not use bare AuthUser — use InvoiceProcessingAccess instead"
    );
}

#[test]
fn test_routing_handlers_use_invoice_processing_access() {
    let source = include_str!("../src/routes/routing.rs");
    assert!(
        source.contains("InvoiceProcessingAccess"),
        "routing.rs must import InvoiceProcessingAccess"
    );
    assert!(
        !source.contains("AuthUser(user): AuthUser"),
        "routing.rs must not use bare AuthUser — use InvoiceProcessingAccess instead"
    );
}

// ---------------------------------------------------------------------------
// Compile-time proof that InvoiceProcessingAccess extractor exists
// ---------------------------------------------------------------------------

#[test]
fn test_invoice_processing_access_extractor_is_importable() {
    use billforge_api::extractors::InvoiceProcessingAccess;
    let _ = std::marker::PhantomData::<InvoiceProcessingAccess>;
}
