//! Integration gating tests: verify that integration modules (QuickBooks, Xero,
//! etc.) are properly gated by tenant subscription state.
//!
//! These tests follow the same pattern as `module_gating_dashboard_notifications_routing.rs`:
//! - TenantContext::has_module(Module::Quickbooks) gates access.
//! - Error::ModuleNotAvailable maps to HTTP 402.
//! - Route wiring includes the middleware layer (source-level check).

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::get,
    Extension, Router,
};
use billforge_api::middleware::{
    require_ai_assistant, require_capture, require_edi, require_processing,
    require_vendor_management,
};
use billforge_core::{Error, Module, TenantContext, TenantId, TenantSettings};
use tower::util::ServiceExt;
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

// ---------------------------------------------------------------------------
// Runtime gate enforcement: exercise gate_module through a real router and
// confirm 402 is actually returned when the tenant lacks the module.
// ---------------------------------------------------------------------------

async fn gate_test_handler() -> &'static str {
    "ok"
}

fn build_gate_test_router(ctx: TenantContext) -> Router {
    Router::new()
        .route("/api/v1/edi/test", get(gate_test_handler))
        .layer(middleware::from_fn(require_edi))
        .layer(Extension(ctx))
}

#[tokio::test]
async fn gate_returns_402_when_module_disabled() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "No EDI Tenant".to_string(),
        enabled_modules: vec![Module::InvoiceCapture],
        settings: TenantSettings::default(),
    };
    let app = build_gate_test_router(ctx);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/edi/test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(
        response.status(),
        StatusCode::PAYMENT_REQUIRED,
        "gate_module must return 402 when tenant lacks the Edi module"
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .expect("collect response body");
    let body: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("response is valid JSON");
    assert_eq!(
        body["error"]["code"], "module_not_entitled",
        "402 body must include the module_not_entitled error code, got: {}",
        body
    );
}

#[tokio::test]
async fn gate_passes_when_module_enabled() {
    let ctx = TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: "EDI Tenant".to_string(),
        enabled_modules: vec![Module::Edi],
        settings: TenantSettings::default(),
    };
    let app = build_gate_test_router(ctx);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/edi/test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_ne!(
        response.status(),
        StatusCode::PAYMENT_REQUIRED,
        "gate_module must let the request through when Edi is enabled"
    );
}

#[test]
fn require_tenant_loads_and_inserts_tenant_context() {
    let source = include_str!("../src/middleware.rs");
    assert!(
        source.contains("auth.get_tenant_context"),
        "require_tenant must call auth.get_tenant_context to load module entitlements"
    );
    assert!(
        source.contains("State(auth): State<Arc<AuthService>>"),
        "require_tenant must accept the AuthService state so it can load TenantContext"
    );
    assert!(
        source.contains("tenant_context_load_failed"),
        "require_tenant must surface a tenant_context_load_failed error code on lookup failure"
    );
}

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

// ---------------------------------------------------------------------------
// Core pillar runtime gates: Capture, Processing, VendorManagement, AiAssistant
// ---------------------------------------------------------------------------

fn tenant_with(modules: Vec<Module>, name: &str) -> TenantContext {
    TenantContext {
        tenant_id: TenantId::from_uuid(Uuid::nil()),
        tenant_name: name.to_string(),
        enabled_modules: modules,
        settings: TenantSettings::default(),
    }
}

async fn status_for(app: Router, uri: &str) -> StatusCode {
    app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .expect("router responds")
        .status()
}

#[tokio::test]
async fn test_capture_module_gated() {
    let ctx = tenant_with(vec![Module::Reporting], "No Capture Tenant");
    let app = Router::new()
        .route("/api/v1/invoice-captures", get(gate_test_handler))
        .layer(middleware::from_fn(require_capture))
        .layer(Extension(ctx));
    assert_eq!(
        status_for(app, "/api/v1/invoice-captures").await,
        StatusCode::PAYMENT_REQUIRED,
        "Capture must be gated when tenant lacks InvoiceCapture module"
    );
}

#[tokio::test]
async fn test_processing_module_gated() {
    let ctx = tenant_with(vec![Module::InvoiceCapture], "No Processing Tenant");
    let app = Router::new()
        .route("/api/v1/invoices", get(gate_test_handler))
        .layer(middleware::from_fn(require_processing))
        .layer(Extension(ctx));
    assert_eq!(
        status_for(app, "/api/v1/invoices").await,
        StatusCode::PAYMENT_REQUIRED,
        "Processing must be gated when tenant lacks InvoiceProcessing module"
    );
}

#[tokio::test]
async fn test_vendor_module_gated() {
    let ctx = tenant_with(vec![Module::InvoiceCapture], "No Vendor Tenant");
    let app = Router::new()
        .route("/api/v1/vendors", get(gate_test_handler))
        .layer(middleware::from_fn(require_vendor_management))
        .layer(Extension(ctx));
    assert_eq!(
        status_for(app, "/api/v1/vendors").await,
        StatusCode::PAYMENT_REQUIRED,
        "Vendors must be gated when tenant lacks VendorManagement module"
    );
}

#[tokio::test]
async fn test_ai_assistant_module_gated() {
    let ctx = tenant_with(vec![Module::InvoiceCapture], "No AI Tenant");
    let app = Router::new()
        .route("/api/v1/ai/chat", get(gate_test_handler))
        .layer(middleware::from_fn(require_ai_assistant))
        .layer(Extension(ctx));
    assert_eq!(
        status_for(app, "/api/v1/ai/chat").await,
        StatusCode::PAYMENT_REQUIRED,
        "AI Assistant must be gated when tenant lacks AiAssistant module"
    );
}

#[tokio::test]
async fn test_core_modules_pass_when_entitled() {
    let ctx = tenant_with(
        vec![
            Module::InvoiceCapture,
            Module::InvoiceProcessing,
            Module::VendorManagement,
            Module::AiAssistant,
        ],
        "All Core Tenant",
    );
    let app = Router::new()
        .route("/api/v1/invoice-captures", get(gate_test_handler))
        .layer(middleware::from_fn(require_capture))
        .merge(
            Router::new()
                .route("/api/v1/invoices", get(gate_test_handler))
                .layer(middleware::from_fn(require_processing)),
        )
        .merge(
            Router::new()
                .route("/api/v1/vendors", get(gate_test_handler))
                .layer(middleware::from_fn(require_vendor_management)),
        )
        .merge(
            Router::new()
                .route("/api/v1/ai/chat", get(gate_test_handler))
                .layer(middleware::from_fn(require_ai_assistant)),
        )
        .layer(Extension(ctx));

    for path in [
        "/api/v1/invoice-captures",
        "/api/v1/invoices",
        "/api/v1/vendors",
        "/api/v1/ai/chat",
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .expect("router responds");
        assert_ne!(
            response.status(),
            StatusCode::PAYMENT_REQUIRED,
            "gate must pass for {} when tenant has the matching module",
            path
        );
    }
}

#[test]
fn test_middleware_defines_core_pillar_gates() {
    let source = include_str!("../src/middleware.rs");
    for name in [
        "pub async fn require_capture",
        "pub async fn require_processing",
        "pub async fn require_vendor_management",
        "pub async fn require_ai_assistant",
    ] {
        assert!(
            source.contains(name),
            "middleware.rs must define {}",
            name
        );
    }
}

#[test]
fn test_routes_mod_applies_core_pillar_gates() {
    let source = include_str!("../src/routes/mod.rs");
    for name in [
        "require_capture",
        "require_processing",
        "require_vendor_management",
        "require_ai_assistant",
    ] {
        assert!(
            source.contains(name),
            "routes/mod.rs must reference {} on a core pillar route mount",
            name
        );
    }
}
