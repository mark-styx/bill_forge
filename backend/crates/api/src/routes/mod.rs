//! API routes

mod audit;
mod auth;
pub mod dashboard;
mod documents;
mod health;
mod invoices;
mod vendors;
mod workflows;
mod reports;
mod export;
mod sandbox;
pub mod email_actions;
pub mod quickbooks;
pub mod xero;
pub mod sage_intacct;
pub mod salesforce;
pub mod workday;
pub mod bill_com;
pub mod edi;
pub mod notifications;
pub mod predictive;
pub mod mobile;
mod settings;
mod feedback;

use crate::middleware::{rate_limit_auth, RateLimiterState};
use crate::state::AppState;
use crate::metrics;
use axum::{middleware, routing::get, Extension, Router};

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
        // API routes
        .nest("/api/v1", api_routes())
        .with_state(state)
}

/// Prometheus metrics endpoint
async fn metrics_handler() -> String {
    metrics::export_metrics()
}

/// Landing page with API info
async fn landing_page() -> axum::response::Html<String> {
    let version = env!("CARGO_PKG_VERSION");
    axum::response::Html(format!(include_str!("../../../../landing.html"), version))
}

/// API v1 routes
fn api_routes() -> Router<AppState> {
    Router::new()
        // Authentication (rate limited: 20 requests per 60 seconds per source IP)
        .nest("/auth", auth::routes()
            .layer(middleware::from_fn(rate_limit_auth))
            .layer(Extension(RateLimiterState::new(20, 60))))
        // Invoice Capture module
        .nest("/invoices", invoices::routes())
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
        // QuickBooks integration
        .nest("/quickbooks", quickbooks::routes())
        // Xero integration
        .nest("/xero", xero::routes())
        // Sage Intacct integration
        .nest("/sage-intacct", sage_intacct::routes())
        // Salesforce CRM integration
        .nest("/salesforce", salesforce::routes())
        // Workday Financial Management integration
        .nest("/workday", workday::routes())
        // Bill.com AP payments integration
        .nest("/bill-com", bill_com::routes())
        // EDI (Electronic Data Interchange)
        .nest("/edi", edi::routes())
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
        // Email actions (approve/reject via email)
        .nest("/actions", email_actions::routes())
}
