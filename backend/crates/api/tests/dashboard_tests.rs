//! Integration tests for Dashboard metrics endpoints
//!
//! Tests the analytics dashboard API endpoints for:
//! - Invoice processing metrics
//! - Approval workflow metrics
//! - Vendor analytics
//! - Team performance metrics

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use billforge_api::{routes, AppState, Config};
use serde_json::Value;
use tower::util::ServiceExt;

/// Helper to create test app state
async fn create_test_state() -> AppState {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    std::env::set_var("DATABASE_URL", "sqlite://:memory:");
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config).await.expect("Failed to create test state")
}

/// Helper to create the test router
async fn create_test_router() -> axum::Router {
    let state = create_test_state().await;
    routes::create_router(state)
}

/// Helper to get auth token (would normally create a test user)
fn get_test_auth_token() -> String {
    // In a real test, you'd generate a valid JWT token
    // For now, we'll test the endpoint structure
    "Bearer test-token".to_string()
}

// ============================================================================
// Dashboard Metrics Tests
// ============================================================================

#[tokio::test]
async fn test_get_dashboard_metrics_requires_auth() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should require authentication
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_invoice_metrics_requires_auth() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard/metrics/invoices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_approval_metrics_requires_auth() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard/metrics/approvals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_vendor_metrics_requires_auth() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard/metrics/vendors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_get_team_metrics_requires_auth() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard/metrics/team")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// Invoice Metrics Structure Tests
// ============================================================================

#[test]
fn test_invoice_metrics_structure() {
    use billforge_api::routes::dashboard::InvoiceMetrics;

    let metrics = InvoiceMetrics {
        total_invoices: 100,
        pending_ocr: 10,
        ready_for_review: 15,
        submitted: 20,
        approved: 45,
        rejected: 5,
        paid: 40,
        avg_processing_time_hours: 2.5,
        total_value: 5000000,
        this_month: 30,
        trend_vs_last_month: 12.5,
    };

    // Verify all fields are accessible
    assert_eq!(metrics.total_invoices, 100);
    assert_eq!(metrics.pending_ocr, 10);
    assert_eq!(metrics.ready_for_review, 15);
    assert_eq!(metrics.submitted, 20);
    assert_eq!(metrics.approved, 45);
    assert_eq!(metrics.rejected, 5);
    assert_eq!(metrics.paid, 40);
    assert_eq!(metrics.avg_processing_time_hours, 2.5);
    assert_eq!(metrics.total_value, 5000000);
    assert_eq!(metrics.this_month, 30);
    assert_eq!(metrics.trend_vs_last_month, 12.5);
}

#[test]
fn test_invoice_metrics_serialization() {
    use billforge_api::routes::dashboard::InvoiceMetrics;

    let metrics = InvoiceMetrics {
        total_invoices: 50,
        pending_ocr: 5,
        ready_for_review: 8,
        submitted: 12,
        approved: 20,
        rejected: 2,
        paid: 18,
        avg_processing_time_hours: 1.5,
        total_value: 2500000,
        this_month: 15,
        trend_vs_last_month: -5.0,
    };

    let json = serde_json::to_string(&metrics).expect("Failed to serialize");
    let deserialized: InvoiceMetrics = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.total_invoices, 50);
    assert_eq!(deserialized.trend_vs_last_month, -5.0);
}

// ============================================================================
// Approval Metrics Structure Tests
// ============================================================================

#[test]
fn test_approval_metrics_structure() {
    use billforge_api::routes::dashboard::ApprovalMetrics;

    let metrics = ApprovalMetrics {
        pending_approvals: 10,
        approved_today: 5,
        rejected_today: 1,
        avg_approval_time_hours: 3.2,
        approval_rate: 85.5,
        escalated: 2,
        overdue: 3,
    };

    assert_eq!(metrics.pending_approvals, 10);
    assert_eq!(metrics.approved_today, 5);
    assert_eq!(metrics.rejected_today, 1);
    assert_eq!(metrics.avg_approval_time_hours, 3.2);
    assert_eq!(metrics.approval_rate, 85.5);
    assert_eq!(metrics.escalated, 2);
    assert_eq!(metrics.overdue, 3);
}

#[test]
fn test_approval_metrics_serialization() {
    use billforge_api::routes::dashboard::ApprovalMetrics;

    let metrics = ApprovalMetrics {
        pending_approvals: 15,
        approved_today: 8,
        rejected_today: 2,
        avg_approval_time_hours: 4.5,
        approval_rate: 90.0,
        escalated: 1,
        overdue: 2,
    };

    let json = serde_json::to_string(&metrics).expect("Failed to serialize");
    let deserialized: ApprovalMetrics = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.approval_rate, 90.0);
    assert_eq!(deserialized.pending_approvals, 15);
}

// ============================================================================
// Vendor Metrics Structure Tests
// ============================================================================

#[test]
fn test_vendor_metrics_structure() {
    use billforge_api::routes::dashboard::{VendorMetrics, TopVendor};

    let metrics = VendorMetrics {
        total_vendors: 50,
        new_this_month: 5,
        top_vendors: vec![
            TopVendor {
                vendor_id: "vendor-1".to_string(),
                vendor_name: "Acme Corp".to_string(),
                invoice_count: 25,
                total_amount: 150000,
            },
        ],
        concentration_percentage: 75.0,
    };

    assert_eq!(metrics.total_vendors, 50);
    assert_eq!(metrics.new_this_month, 5);
    assert_eq!(metrics.top_vendors.len(), 1);
    assert_eq!(metrics.top_vendors[0].vendor_name, "Acme Corp");
    assert_eq!(metrics.concentration_percentage, 75.0);
}

// ============================================================================
// Team Metrics Structure Tests
// ============================================================================

#[test]
fn test_team_metrics_structure() {
    use billforge_api::routes::dashboard::{TeamMetrics, TeamMemberStats};

    let metrics = TeamMetrics {
        members: vec![
            TeamMemberStats {
                user_id: "user-1".to_string(),
                user_name: "Alice Johnson".to_string(),
                approvals_this_month: 45,
                rejections_this_month: 3,
                avg_response_time_hours: 2.5,
            },
        ],
        avg_approvals_per_member: 10.5,
        total_pending_actions: 15,
    };

    assert_eq!(metrics.members.len(), 1);
    assert_eq!(metrics.members[0].user_name, "Alice Johnson");
    assert_eq!(metrics.avg_approvals_per_member, 10.5);
    assert_eq!(metrics.total_pending_actions, 15);
}

// ============================================================================
// Dashboard Metrics Aggregation Tests
// ============================================================================

#[test]
fn test_dashboard_metrics_aggregation() {
    use billforge_api::routes::dashboard::{
        DashboardMetrics, InvoiceMetrics, ApprovalMetrics, VendorMetrics, TeamMetrics,
    };

    let dashboard = DashboardMetrics {
        invoices: InvoiceMetrics {
            total_invoices: 100,
            pending_ocr: 10,
            ready_for_review: 15,
            submitted: 20,
            approved: 45,
            rejected: 5,
            paid: 40,
            avg_processing_time_hours: 2.5,
            total_value: 5000000,
            this_month: 30,
            trend_vs_last_month: 12.5,
        },
        approvals: ApprovalMetrics {
            pending_approvals: 10,
            approved_today: 5,
            rejected_today: 1,
            avg_approval_time_hours: 3.2,
            approval_rate: 85.5,
            escalated: 2,
            overdue: 3,
        },
        vendors: VendorMetrics {
            total_vendors: 50,
            new_this_month: 5,
            top_vendors: vec![],
            concentration_percentage: 75.0,
        },
        team: TeamMetrics {
            members: vec![],
            avg_approvals_per_member: 10.5,
            total_pending_actions: 15,
        },
    };

    // Verify nested structures
    assert_eq!(dashboard.invoices.total_invoices, 100);
    assert_eq!(dashboard.approvals.pending_approvals, 10);
    assert_eq!(dashboard.vendors.total_vendors, 50);
    assert_eq!(dashboard.team.total_pending_actions, 15);

    // Verify JSON serialization
    let json = serde_json::to_string(&dashboard).expect("Failed to serialize dashboard");
    let deserialized: DashboardMetrics =
        serde_json::from_str(&json).expect("Failed to deserialize dashboard");

    assert_eq!(deserialized.invoices.total_invoices, 100);
    assert_eq!(deserialized.approvals.approval_rate, 85.5);
}
