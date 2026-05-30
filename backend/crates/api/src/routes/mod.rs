//! API routes

pub mod ai;
pub mod approval_links;
pub(crate) mod audit;
pub mod auth;
#[cfg(feature = "bill-com")]
pub mod bill_com;
pub mod billing;
pub mod chat_approvals;
pub mod close_periods;
pub mod dashboard;
pub mod discounts;
pub(crate) mod documents;
#[cfg(feature = "edi")]
pub mod edi;
pub mod email_actions;
pub(crate) mod export;
pub(crate) mod feedback;
pub mod health;
pub mod implementation;
pub mod inbound_email;
pub mod invoices;
pub mod mobile;
pub mod notifications;
pub mod predictive;
#[cfg(feature = "edi")]
pub mod purchase_orders;
pub mod qbo;
#[cfg(feature = "quickbooks")]
pub mod quickbooks;
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
pub mod vendor_statements;
pub(crate) mod vendors;
#[cfg(feature = "workday")]
pub mod workday;
pub(crate) mod workflows;
#[cfg(feature = "xero")]
pub mod xero;

use crate::metrics;
use crate::middleware::{rate_limit_auth, require_auth, require_tenant, RateLimiterState};
use crate::state::AppState;
use axum::{middleware, routing::get, Extension, Router};
use std::time::Instant;

/// Create the main API router
pub fn create_router(state: AppState) -> Router {
    // Initialize health check start time
    health::init_start_time();

    Router::new()
        // Root landing page
        .route("/", get(landing_page))
        // Health check endpoints
        .route("/health", get(health::health_check))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .route("/health/detailed", get(health::detailed_health))
        .route("/health/ocr", get(health::ocr_health))
        // Prometheus metrics endpoint
        .route("/metrics", get(metrics_handler))
        // Inbound email webhook (no auth — uses shared secret header)
        .nest("/webhooks", inbound_email::routes())
        // Chat approval surface — Slack/Teams interaction callbacks (no JWT; verified via signing secret)
        // NOTE: Teams /teams/actions is disabled by default (TEAMS_ACTIONS_ENABLED). See chat_approvals.rs.
        .nest("/integrations", chat_approvals::routes())
        // API routes
        .nest("/api/v1", api_routes(state.clone()))
        .with_state(state)
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
        // Invoice Capture module + status state machine transitions
        .nest(
            "/invoices",
            invoices::routes().merge(crate::state_machine::routes()),
        )
        // Vendor Management module
        .nest("/vendors", vendors::routes())
        // Invoice Processing module
        .nest("/workflows", workflows::routes())
        // Reporting module
        .nest("/reports", reports::routes())
        // Dashboard metrics
        .nest("/dashboard", dashboard::routes())
        // Data export
        .nest("/export", export::routes())
        // Document storage
        .nest("/documents", documents::routes())
        // Audit logs
        .nest("/audit", audit::routes())
        // Sandbox/Development persona management
        .nest("/sandbox", sandbox::routes())
        // Server-backed implementation wizard
        .nest("/implementation", implementation::routes());
    // Conditionally include ERP/integration routes
    let router = {
        #[cfg(feature = "quickbooks")]
        let router = router.nest("/quickbooks", quickbooks::routes());
        #[cfg(feature = "xero")]
        let router = router.nest("/xero", xero::routes());
        #[cfg(feature = "sage-intacct")]
        let router = router.nest("/sage-intacct", sage_intacct::routes());
        #[cfg(feature = "salesforce")]
        let router = router.nest("/salesforce", salesforce::routes());
        #[cfg(feature = "workday")]
        let router = router.nest("/workday", workday::routes());
        #[cfg(feature = "bill-com")]
        let router = router.nest("/bill-com", bill_com::routes());
        #[cfg(feature = "edi")]
        let router = router.nest("/edi", edi::routes());
        #[cfg(feature = "edi")]
        let router = router.nest("/edi/purchase-orders", purchase_orders::routes());
        router
    };
    router
        // Notifications (Slack/Teams)
        .nest("/notifications", notifications::routes())
        // Predictive Analytics (Forecasting & Anomaly Detection)
        .nest("/analytics/predictive", predictive::routes())
        // Mobile App Backend (Device management, dashboard, approvals)
        .nest("/mobile", mobile::routes())
        // Tenant settings
        .nest("/settings", settings::routes())
        // User feedback
        .nest("/feedback", feedback::routes())
        // Organization theme
        .nest("/organization/theme", theme::org_routes())
        // User theme preferences
        .nest("/user/theme", theme::user_routes())
        // Email actions (approve/reject via email)
        .nest("/actions", email_actions::routes())
        // AI Assistant (Winston)
        .nest("/ai", ai::routes())
        // Billing & Subscription
        .nest("/billing", billing::routes())
        // Vendor Statement Reconciliation
        .merge(vendor_statements::routes())
        // Intelligent Routing & Workload Balancing
        .nest("/routing", routing::routes())
        // Approval magic links (email-based approve/reject/comment)
        .nest("/approval-links", approval_links::routes())
        // Vendor self-service portal (public, validates own vendor-portal JWT)
        .nest("/vendor-portal", vendor_portal::routes())
        // Lightweight QBO integration (OAuth + vendor pull)
        .nest("/qbo", qbo::routes())
        // Month-end close periods
        .nest("/close-periods", close_periods::routes())
        // Early-payment discount optimizer
        .nest("/discounts", discounts::routes())
        // Invoice Capture (standalone OCR upload)
        .nest("/invoice-captures", crate::invoice_capture::routes())
        // Validate JWT on all API routes (public paths are exempted inside the middleware)
        .layer(middleware::from_fn(require_tenant))
        .layer(middleware::from_fn_with_state(state, require_auth))
        // SLO telemetry: duration + sub-200ms compliance for every API request
        .layer(middleware::from_fn(track_http_slo))
}
