//! Integration gating tests: verify that integration modules (QuickBooks, Xero,
//! etc.) are properly gated by tenant subscription state.
//!
//! These tests follow the same pattern as `module_gating_dashboard_notifications_routing.rs`:
//! - TenantContext::has_module(Module::Quickbooks) gates access.
//! - Error::ModuleNotAvailable maps to HTTP 402.
//! - Route wiring includes the middleware layer (source-level check).

use billforge_core::{Error, Module, TenantContext, TenantId, TenantSettings};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// TenantContext gating logic
// ---------------------------------------------------------------------------

#[test]
fn test_tenant_without_quickbooks_cannot_access_integration() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "No Integrations Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture, Module::Reporting],
        settings: TenantSettings::default(),
    };
    assert!(
        !ctx.has_module(Module::Quickbooks),
        "Tenant without Quickbooks add-on must not pass the module check"
    );
    assert!(
        !ctx.has_module(Module::Xero),
        "Tenant without Xero add-on must not pass the module check"
    );
}

#[test]
fn test_tenant_with_quickbooks_can_access() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "QuickBooks Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture, Module::Quickbooks],
        settings: TenantSettings::default(),
    };
    assert!(
        ctx.has_module(Module::Quickbooks),
        "Tenant with Quickbooks add-on must pass the module check"
    );
    // But not Xero — per-module gating, not blanket integration access.
    assert!(
        !ctx.has_module(Module::Xero),
        "Quickbooks tenant must not have Xero access"
    );
}

#[test]
fn test_tenant_with_xero_can_access() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "Xero Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture, Module::Xero],
        settings: TenantSettings::default(),
    };
    assert!(
        ctx.has_module(Module::Xero),
        "Tenant with Xero add-on must pass the module check"
    );
    assert!(
        !ctx.has_module(Module::Quickbooks),
        "Xero tenant must not have Quickbooks access"
    );
}

// ---------------------------------------------------------------------------
// Error contract: ModuleNotAvailable → 402
// ---------------------------------------------------------------------------

#[test]
fn test_module_not_available_integration_maps_to_402() {
    let err = Error::ModuleNotAvailable("QuickBooks Online".to_string());
    assert_eq!(
        err.status_code(),
        402,
        "ModuleNotAvailable must be HTTP 402"
    );
    assert_eq!(err.error_code(), "MODULE_NOT_AVAILABLE");
}

// ---------------------------------------------------------------------------
// Route wiring guards: verify middleware import in mod.rs
// ---------------------------------------------------------------------------

#[test]
fn test_routes_mod_imports_quickbooks_gating_middleware() {
    let source = include_str!("../src/routes/mod.rs");
    assert!(
        source.contains("require_quickbooks"),
        "routes/mod.rs must import and use require_quickbooks middleware"
    );
    assert!(
        source.contains("require_xero"),
        "routes/mod.rs must import and use require_xero middleware"
    );
}

#[test]
fn test_middleware_defines_integration_gates() {
    let source = include_str!("../src/middleware.rs");
    assert!(
        source.contains("pub async fn require_quickbooks"),
        "middleware.rs must define require_quickbooks"
    );
    assert!(
        source.contains("pub async fn require_xero"),
        "middleware.rs must define require_xero"
    );
    assert!(
        source.contains("pub async fn require_edi"),
        "middleware.rs must define require_edi"
    );
    assert!(
        source.contains("pub async fn require_netsuite"),
        "middleware.rs must define require_netsuite"
    );
    assert!(
        source.contains("module_not_entitled"),
        "middleware.rs must reference module_not_entitled error code"
    );
}

// ---------------------------------------------------------------------------
// Module round-trip: ensure integration modules parse from DB/API strings
// ---------------------------------------------------------------------------

#[test]
fn test_integration_module_from_str_round_trips() {
    let cases = [
        ("quickbooks", Module::Quickbooks),
        ("xero", Module::Xero),
        ("net_suite", Module::NetSuite),
        ("sage_intacct", Module::SageIntacct),
        ("salesforce", Module::Salesforce),
        ("workday", Module::Workday),
        ("bill_com", Module::BillCom),
        ("edi", Module::Edi),
    ];
    for (slug, expected) in &cases {
        let parsed: Module = slug
            .parse()
            .unwrap_or_else(|e| panic!("Failed to parse {:?}: {}", slug, e));
        assert_eq!(parsed, *expected);
        assert_eq!(expected.as_str(), *slug);
    }
}

// ---------------------------------------------------------------------------
// NetSuite route mount guard
// ---------------------------------------------------------------------------

#[test]
fn test_routes_mod_mounts_netsuite() {
    let source = include_str!("../src/routes/mod.rs");
    assert!(
        source.contains("pub mod netsuite"),
        "routes/mod.rs must declare the netsuite module"
    );
    assert!(
        source.contains("\"/netsuite\""),
        "routes/mod.rs must mount the /netsuite path"
    );
    assert!(
        source.contains("require_netsuite"),
        "routes/mod.rs must apply require_netsuite middleware to the netsuite route"
    );
}
