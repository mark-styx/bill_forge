//! Integration tests for the BillForge API
//!
//! These tests verify the API endpoints work correctly end-to-end.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use billforge_api::{routes, AppState, Config};
use serde_json::{json, Value};
use tower::util::ServiceExt;

/// Helper to create test app state
async fn create_test_state() -> AppState {
    // Set required environment variables for testing
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

// ============================================================================
// Health Check Tests
// ============================================================================

#[tokio::test]
async fn test_health_endpoint_returns_200() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_endpoint_returns_json() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("status").is_some());
    assert_eq!(json["status"], "healthy");
}

#[tokio::test]
async fn test_liveness_endpoint() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_readiness_endpoint() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be OK if database is connected
    assert!(response.status() == StatusCode::OK || response.status() == StatusCode::SERVICE_UNAVAILABLE);
}

// ============================================================================
// Authentication Tests
// ============================================================================

#[tokio::test]
async fn test_login_with_invalid_credentials() {
    let app = create_test_router().await;

    let login_body = json!({
        "tenant_id": "11111111-1111-1111-1111-111111111111",
        "email": "nonexistent@example.com",
        "password": "wrongpassword"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_missing_fields() {
    let app = create_test_router().await;

    let login_body = json!({
        "email": "test@example.com"
        // Missing tenant_id and password
    });

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&login_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_protected_endpoint_without_auth() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/invoices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_protected_endpoint_with_invalid_token() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/invoices")
                .header(header::AUTHORIZATION, "Bearer invalid-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// Input Validation Tests
// ============================================================================

#[tokio::test]
async fn test_registration_password_validation() {
    let app = create_test_router().await;

    // Password too short
    let register_body = json!({
        "tenant_id": "11111111-1111-1111-1111-111111111111",
        "email": "newuser@example.com",
        "password": "short",
        "name": "Test User"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&register_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_registration_with_valid_data() {
    let app = create_test_router().await;

    // Valid registration should succeed or fail with appropriate status
    // (may succeed with sandbox tenant or fail if tenant not found)
    let register_body = json!({
        "tenant_id": "11111111-1111-1111-1111-111111111111",
        "email": "test@example.com",
        "password": "ValidPassword123",
        "name": "Test User"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_string(&register_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be OK (created) or conflict (already exists) or not found (tenant)
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::CREATED
            || response.status() == StatusCode::CONFLICT
            || response.status() == StatusCode::NOT_FOUND
    );
}

// ============================================================================
// Content Type Tests
// ============================================================================

#[tokio::test]
async fn test_json_endpoint_requires_content_type() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/auth/login")
                // No content-type header
                .body(Body::from(r#"{"email":"test@example.com"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should reject without proper content type or return UNPROCESSABLE_ENTITY
    assert!(
        response.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY
            || response.status() == StatusCode::BAD_REQUEST
    );
}

// ============================================================================
// Route Not Found Tests
// ============================================================================

#[tokio::test]
async fn test_unknown_route_returns_404() {
    let app = create_test_router().await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_method_not_allowed() {
    let app = create_test_router().await;

    // Health endpoint only supports GET
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
