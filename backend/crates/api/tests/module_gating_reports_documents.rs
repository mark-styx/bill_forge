//! Integration tests verifying that Reports and Documents routes are gated
//! by the `ReportingAccess` and `DocumentsAccess` extractors respectively.
//!
//! These tests mirror the approach in `module_gating_dashboard_notifications_routing.rs`:
//! - Verify the extractor import exists in the route source (compile-time wiring).
//! - Verify `TenantContext::has_module(...)` gates access.
//! - Verify `Error::ModuleNotAvailable` maps to HTTP 402.

use billforge_core::{Error, Module, TenantContext, TenantId, TenantSettings};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Reports gating logic
// ---------------------------------------------------------------------------

#[test]
fn test_tenant_without_reporting_cannot_access_reports() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Capture Only Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture],
        settings: TenantSettings::default(),
    };
    assert!(
        !ctx.has_module(Module::Reporting),
        "Tenant with only InvoiceCapture must not have Reporting"
    );
}

#[test]
fn test_tenant_with_reporting_can_access_reports() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Reporting Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture, Module::Reporting],
        settings: TenantSettings::default(),
    };
    assert!(
        ctx.has_module(Module::Reporting),
        "Tenant with Reporting enabled must pass the check"
    );
}

// ---------------------------------------------------------------------------
// Documents gating logic (gated on InvoiceCapture)
// ---------------------------------------------------------------------------

#[test]
fn test_tenant_without_invoice_capture_cannot_access_documents() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "No Modules Tenant".to_string(),
        enabled_modules: vec![],
        settings: TenantSettings::default(),
    };
    assert!(
        !ctx.has_module(Module::InvoiceCapture),
        "Tenant with no modules must not have InvoiceCapture (Documents gate)"
    );
}

#[test]
fn test_tenant_with_invoice_capture_can_access_documents() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Capture Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture],
        settings: TenantSettings::default(),
    };
    assert!(
        ctx.has_module(Module::InvoiceCapture),
        "Tenant with InvoiceCapture must pass the Documents gate"
    );
}

// ---------------------------------------------------------------------------
// Error contract: ModuleNotAvailable → 402
// ---------------------------------------------------------------------------

#[test]
fn test_module_not_available_reporting_maps_to_402() {
    let err = Error::ModuleNotAvailable("Reporting".to_string());
    assert_eq!(err.status_code(), 402, "ModuleNotAvailable must be HTTP 402");
    assert_eq!(err.error_code(), "MODULE_NOT_AVAILABLE");
}

#[test]
fn test_module_not_available_documents_maps_to_402() {
    let err = Error::ModuleNotAvailable("Documents".to_string());
    assert_eq!(err.status_code(), 402, "ModuleNotAvailable must be HTTP 402");
    assert_eq!(err.error_code(), "MODULE_NOT_AVAILABLE");
}

// ---------------------------------------------------------------------------
// Route wiring guards: verify extractor import in source files
// ---------------------------------------------------------------------------

#[test]
fn test_reports_handlers_use_reporting_access() {
    let source = include_str!("../src/routes/reports.rs");
    assert!(
        source.contains("ReportingAccess"),
        "reports.rs must import ReportingAccess"
    );
    // No handler should use bare TenantCtx after the fix
    assert!(
        !source.contains("TenantCtx(tenant): TenantCtx"),
        "reports.rs must not use bare TenantCtx — use ReportingAccess instead"
    );
    assert!(
        !source.contains("AuthUser(user): AuthUser"),
        "reports.rs must not use bare AuthUser — use ReportingAccess instead"
    );
}

#[test]
fn test_documents_handlers_use_documents_access() {
    let source = include_str!("../src/routes/documents.rs");
    assert!(
        source.contains("DocumentsAccess"),
        "documents.rs must import DocumentsAccess"
    );
    assert!(
        !source.contains("TenantCtx(tenant): TenantCtx"),
        "documents.rs must not use bare TenantCtx — use DocumentsAccess instead"
    );
    assert!(
        !source.contains("AuthUser(user): AuthUser")
            && !source.contains("AuthUser(_user): AuthUser"),
        "documents.rs must not use bare AuthUser — use DocumentsAccess instead"
    );
}

// ---------------------------------------------------------------------------
// Compile-time proof that extractors exist
// ---------------------------------------------------------------------------

#[test]
fn test_reporting_access_extractor_is_importable() {
    use billforge_api::extractors::ReportingAccess;
    let _ = std::marker::PhantomData::<ReportingAccess>;
}

#[test]
fn test_documents_access_extractor_is_importable() {
    use billforge_api::extractors::DocumentsAccess;
    let _ = std::marker::PhantomData::<DocumentsAccess>;
}
