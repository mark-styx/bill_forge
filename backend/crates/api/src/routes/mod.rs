//! API routes

#[cfg(feature = "ai-agent")]
pub mod ai;
#[cfg(feature = "analytics")]
pub mod analytics;
pub mod ap_command_center;
pub mod ap_migration;
pub mod approval_links;
pub(crate) mod audit;
pub mod audit_bundle;
pub mod auth;
pub mod autopilot;
#[cfg(feature = "bill-com")]
pub mod bill_com;
#[cfg(feature = "billing")]
pub mod billing;
pub mod budgets;
#[cfg(feature = "ai-agent")]
pub mod chat_approvals;
pub mod close_periods;
#[cfg(feature = "processing")]
pub mod contracts;
pub mod dashboard;
pub mod discounts;
#[cfg(feature = "ai-agent")]
pub(crate) mod document_qa;
pub(crate) mod documents;
#[cfg(feature = "edi")]
pub mod edi;
pub mod email_actions;
pub(crate) mod erp_jobs;
#[cfg(feature = "processing")]
pub mod explain;
pub(crate) mod export;
pub mod federated_vendor_risk;
pub(crate) mod feedback;
pub mod health;
#[cfg(all(
    feature = "capture",
    feature = "processing",
    feature = "analytics",
    feature = "billing",
    feature = "quickbooks",
    feature = "xero"
))]
pub mod implementation;
pub mod inbound_email;
#[cfg(all(feature = "capture", feature = "processing"))]
pub mod inbox_addin;
#[cfg(all(
    feature = "capture",
    feature = "processing",
    feature = "analytics",
    feature = "billing"
))]
pub mod invoices;
#[cfg(feature = "processing")]
pub mod learning;
pub mod mobile;
#[cfg(feature = "netsuite")]
pub mod netsuite;
pub mod notifications;
pub mod policies;
#[cfg(feature = "analytics")]
pub mod predictive;
pub mod public_api;
#[cfg(feature = "billing")]
pub mod public_signup;
#[cfg(feature = "edi")]
pub mod purchase_orders;
pub mod qbo;
#[cfg(feature = "quickbooks")]
pub mod quickbooks;
pub mod recurring_patterns;
#[cfg(feature = "reporting")]
pub(crate) mod reports;
pub mod routing;
#[cfg(feature = "sage-intacct")]
pub mod sage_intacct;
#[cfg(feature = "salesforce")]
pub mod salesforce;
pub(crate) mod sandbox;
pub(crate) mod settings;
pub mod theme;
pub mod vendor_portal;
pub mod vendor_portal_onboarding;
pub mod vendor_risk_alerts;
pub mod vendor_statements;
pub(crate) mod vendors;
#[cfg(feature = "workday")]
pub mod workday;
pub(crate) mod workflows;
#[cfg(feature = "xero")]
pub mod xero;

use crate::metrics;
use crate::middleware::{
    rate_limit_auth, require_auth, require_bill_com, require_edi, require_netsuite,
    require_quickbooks, require_reporting, require_sage_intacct, require_salesforce,
    require_tenant, require_workday, require_xero, RateLimiterState,
};
use crate::routes::public_api::PublicApiRateLimiter;
use crate::state::AppState;
use axum::{middleware, routing::get, Extension, Router};
use std::time::Instant;

/// Create the main API router
pub fn create_router(state: AppState) -> Router {
    // Initialize health check start time
    health::init_start_time();

    let router = Router::new()
        // Root landing page
        .route("/", get(landing_page))
        // Health check endpoints
        .route("/health", get(health::health_check))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route("/health/detailed", get(health::detailed_health))
        // Prometheus metrics endpoint
        .route("/metrics", get(metrics_handler))
        // Inbound email webhook (no auth — uses shared secret header)
        .nest("/webhooks", inbound_email::routes())
        // API routes
        .nest("/api/v1", api_routes(state.clone()))
        // Public API (PAT auth, not session JWT) — rate limiter attached via extension
        .nest(
            "/api/external/v1",
            public_api::router()
                .layer(Extension(PublicApiRateLimiter(
                    billforge_core::public_api::RateLimiter::new(),
                )))
                .layer(Extension(state.clone())),
        );
    // Pillar-gated top-level mounts. Each is only compiled in when its Cargo
    // feature is enabled, so a single-pillar binary can drop the others.
    let router = {
        // OCR-specific health probe (requires the Capture pillar)
        #[cfg(feature = "capture")]
        let router = router.route("/health/ocr", get(health::ocr_health));
        // Chat approval surface — Slack/Teams interaction callbacks (no JWT; verified via signing secret)
        // NOTE: Teams /teams/actions is disabled by default (TEAMS_ACTIONS_ENABLED). See chat_approvals.rs.
        #[cfg(feature = "ai-agent")]
        let router = router.nest("/integrations", chat_approvals::routes());
        // Stripe webhook - bypasses tenant auth (tenant identity from session metadata)
        #[cfg(feature = "billing")]
        let router = router.nest("/api/v1/billing", billing::public_routes());
        // Self-serve signup + pricing plans (no auth)
        #[cfg(feature = "billing")]
        let router = router.nest("/api/public", public_signup::public_routes());
        router
    };
    // Sandbox promote endpoint (authenticated, inside the auth-gated tree below)
    // Mounted before the api_routes() call so it can be merged in later if needed.
    router.with_state(state.clone())
}

/// Prometheus metrics endpoint
async fn metrics_handler() -> String {
    metrics::export_metrics()
}

/// Middleware that records request-level SLO telemetry (duration + sub-200ms compliance).
async fn track_http_slo(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let start = Instant::now();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();

    // Sanitize endpoint label: collapse UUIDs and numeric IDs to keep cardinality bounded.
    let endpoint = sanitize_endpoint(&path);
    let status = response.status().as_u16();

    metrics::record_request_slo(&endpoint, status, elapsed);

    response
}

/// Collapse path segments that look like UUIDs or numeric IDs so metric labels
/// stay low-cardinality (e.g. `/api/v1/invoices/123` -> `/api/v1/invoices/:id`).
fn sanitize_endpoint(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for seg in path.split('/') {
        if seg.is_empty() {
            out.push('/');
            continue;
        }
        if seg.len() == 36 && seg.matches('-').count() == 4 {
            // Looks like a UUID
            out.push_str("/:id");
        } else if seg.bytes().all(|b| b.is_ascii_digit()) {
            out.push_str("/:id");
        } else {
            out.push_str(seg);
        }
        out.push('/');
    }
    // Remove trailing slash (unless the path is just "/")
    if out.len() > 1 {
        out.pop();
    }
    out
}

/// Landing page with API info
async fn landing_page() -> axum::response::Html<String> {
    let version = env!("CARGO_PKG_VERSION");
    axum::response::Html(format!(include_str!("../../../../landing.html"), version))
}

/// API v1 routes
fn api_routes(state: AppState) -> Router<AppState> {
    let router = Router::new()
        // Authentication (rate limited: 20 requests per 60 seconds per source IP)
        .nest(
            "/auth",
            auth::routes()
                .layer(middleware::from_fn(rate_limit_auth))
                .layer(Extension(RateLimiterState::new(20, 60))),
        )
        // Vendor Management module
        .nest(
            "/vendors",
            vendors::routes()
                .merge(vendor_portal_onboarding::review_routes())
                .merge(vendor_risk_alerts::routes())
                .merge(federated_vendor_risk::vendor_routes()),
        )
        // Federated Vendor Risk Network (#408): tenant consent toggles
        .merge(federated_vendor_risk::tenant_routes())
        // Dashboard metrics
        .nest("/dashboard", dashboard::routes())
        // AP Command Center (standup view)
        .nest("/dashboard/ap-command-center", ap_command_center::routes())
        // AP-to-AP migration importer (BILL.com / Coupa bundle ingest)
        .nest("/migrate/ap", ap_migration::routes())
        // Data export
        .nest("/export", export::routes())
        // Audit logs + evidence bundle export
        .nest("/audit", audit::routes().merge(audit_bundle::routes()))
        // Sandbox/Development persona management
        .nest("/sandbox", sandbox::routes());
    // Pillar-gated route mounts. Each is only compiled in when its Cargo
    // feature is enabled, so a single-pillar binary can drop the others.
    let router = {
        // Invoice Capture + Processing module and status state machine transitions
        #[cfg(all(
            feature = "capture",
            feature = "processing",
            feature = "analytics",
            feature = "billing"
        ))]
        let router = router.nest(
            "/invoices",
            invoices::routes().merge(crate::state_machine::routes()),
        );
        // Invoice Processing module (approval workflows); billing meter events
        // are emitted only when the billing pillar is compiled in.
        let router = router.nest("/workflows", workflows::routes());
        // Reporting module
        #[cfg(feature = "reporting")]
        let router = router.nest("/reports", reports::routes());
        // Document storage (+ Q&A when the AI agent pillar is enabled)
        #[cfg(feature = "ai-agent")]
        let router = router.nest(
            "/documents",
            documents::routes().merge(document_qa::routes()),
        );
        #[cfg(not(feature = "ai-agent"))]
        let router = router.nest("/documents", documents::routes());
        // Server-backed implementation wizard (needs capture + ERP sync + invoice upload types)
        #[cfg(all(
            feature = "capture",
            feature = "processing",
            feature = "analytics",
            feature = "billing",
            feature = "quickbooks",
            feature = "xero"
        ))]
        let router = router.nest("/implementation", implementation::routes());
        router
    };
    // Conditionally include ERP/integration routes, gated by tenant subscription.
    // Compile-time Cargo features control code inclusion; the route_layer gates
    // access at runtime so only tenants with the matching Module add-on can reach
    // the integration handlers.
    let router = {
        #[cfg(feature = "quickbooks")]
        let router = router.nest(
            "/quickbooks",
            quickbooks::routes().layer(middleware::from_fn(require_quickbooks)),
        );
        #[cfg(feature = "xero")]
        let router = router.nest(
            "/xero",
            xero::routes().layer(middleware::from_fn(require_xero)),
        );
        #[cfg(feature = "sage-intacct")]
        let router = router.nest(
            "/sage-intacct",
            sage_intacct::routes().layer(middleware::from_fn(require_sage_intacct)),
        );
        #[cfg(feature = "salesforce")]
        let router = router.nest(
            "/salesforce",
            salesforce::routes().layer(middleware::from_fn(require_salesforce)),
        );
        #[cfg(feature = "workday")]
        let router = router.nest(
            "/workday",
            workday::routes().layer(middleware::from_fn(require_workday)),
        );
        #[cfg(feature = "bill-com")]
        let router = router.nest(
            "/bill-com",
            bill_com::routes().layer(middleware::from_fn(require_bill_com)),
        );
        #[cfg(feature = "edi")]
        let router = router.nest(
            "/edi",
            edi::routes().layer(middleware::from_fn(require_edi)),
        );
        #[cfg(feature = "edi")]
        let router = router.nest(
            "/edi/purchase-orders",
            purchase_orders::routes().layer(middleware::from_fn(require_edi)),
        );
        #[cfg(feature = "netsuite")]
        let router = router.nest(
            "/netsuite",
            netsuite::routes().layer(middleware::from_fn(require_netsuite)),
        );
        router
    };
    let router = router
        // Notifications (Slack/Teams)
        .nest("/notifications", notifications::routes())
        // Mobile App Backend (Device management, dashboard, approvals)
        .nest(
            "/mobile",
            mobile::routes().merge(approval_links::mobile_routes()),
        )
        // Tenant settings
        .nest("/settings", settings::routes())
        // User feedback
        .nest("/feedback", feedback::routes())
        // Organization theme
        .nest("/organization/theme", theme::org_routes())
        // User theme preferences
        .nest("/user/theme", theme::user_routes())
        // Email actions (approve/reject via email) — rate limited: 30 req / 60 s per IP to prevent brute force of opaque tokens
        .nest(
            "/actions",
            email_actions::routes()
                .layer(middleware::from_fn(rate_limit_auth))
                .layer(Extension(RateLimiterState::new(30, 60))),
        )
        // Vendor Statement Reconciliation
        .merge(vendor_statements::routes())
        // Intelligent Routing & Workload Balancing
        .nest("/routing", routing::routes())
        // Approval magic links — rate limited: 30 req / 60 s per IP to prevent brute force of opaque tokens
        .nest(
            "/approval-links",
            approval_links::routes()
                .layer(middleware::from_fn(rate_limit_auth))
                .layer(Extension(RateLimiterState::new(30, 60))),
        )
        // Vendor self-service portal — rate limited: 30 req / 60 s per IP to prevent brute force of opaque tokens
        .nest(
            "/vendor-portal",
            vendor_portal::routes()
                .merge(vendor_portal_onboarding::portal_routes())
                .layer(middleware::from_fn(rate_limit_auth))
                .layer(Extension(RateLimiterState::new(30, 60))),
        )
        // Lightweight QBO integration (OAuth + vendor pull)
        .nest("/qbo", qbo::routes())
        // Month-end close periods
        .nest("/close-periods", close_periods::routes())
        // Early-payment discount optimizer
        .nest("/discounts", discounts::routes())
        // Budget guardrails (finance budget configuration & checks)
        .nest("/budgets", budgets::routes())
        // Recurring-pattern detection & auto-approval policies
        .nest("/recurring-patterns", recurring_patterns::routes())
        // Natural-language policy composer (gated on InvoiceProcessing module via extractors)
        .nest("/policies", policies::routes())
        // Exception-Only Autopilot Cockpit (gated on InvoiceCapture module via extractors)
        .nest("/autopilot", autopilot::routes());
    // Continuous learning panel + correction ingestion (#404).
    #[cfg(feature = "processing")]
    let router = router.nest("/learning", learning::routes());
    // AI Decision Explainability ("Show Your Work") panel (#409).
    #[cfg(feature = "processing")]
    let router = router.nest("/explain", explain::routes());
    // Inbox-Native AP (#406) — Outlook / Gmail add-in JSON surface.
    #[cfg(all(feature = "capture", feature = "processing"))]
    let router = router.nest("/addin", inbox_addin::routes());
    // Pillar-gated mounts for the remainder of the API surface.
    let router = {
        // Predictive Analytics (Forecasting & Anomaly Detection) — gated on Reporting module
        #[cfg(feature = "analytics")]
        let router = router.nest(
            "/analytics/predictive",
            predictive::routes().layer(middleware::from_fn(require_reporting)),
        );
        // Analytics (Usage, Performance, Trends - tenant-scoped via AuthUser)
        #[cfg(feature = "analytics")]
        let router = router.nest("/analytics", analytics::routes());
        // Analytics - Benchmark (opt-in peer insights)
        #[cfg(feature = "analytics")]
        let router = router.nest("/analytics/benchmark", analytics::benchmark_routes());
        // AI Assistant (Winston)
        #[cfg(feature = "ai-agent")]
        let router = router.nest("/ai", ai::routes());
        // Billing & Subscription
        #[cfg(feature = "billing")]
        let router = router.nest("/billing", billing::routes());
        // Self-serve sandbox promotion (authenticated)
        #[cfg(feature = "billing")]
        let router = router.nest("/public", public_signup::promote_route());
        // Contracts (non-PO recurring spend matching)
        #[cfg(feature = "processing")]
        let router = router.nest("/contracts", contracts::routes());
        // Invoice Capture (standalone OCR upload)
        #[cfg(feature = "capture")]
        let router = router.nest("/invoice-captures", crate::invoice_capture::routes());
        router
    };
    router
        // Validate JWT on all API routes (public paths are exempted inside the middleware)
        .layer(middleware::from_fn_with_state(state.clone(), require_tenant))
        .layer(middleware::from_fn_with_state(state, require_auth))
        // SLO telemetry: duration + sub-200ms compliance for every API request
        .layer(middleware::from_fn(track_http_slo))
}
