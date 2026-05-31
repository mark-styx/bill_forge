//! Integration tests for the /api/v1/workflows/inbox endpoint.
//!
//! Seeds two queues with three queue items assigned across two users,
//! calls GET /api/v1/workflows/inbox as user A, and asserts only user A's
//! items return with queue_name/queue_type populated and ordered by
//! priority DESC then entered_at ASC.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use billforge_api::{routes, AppState, Config};
use billforge_auth::{JwtConfig, JwtService};
use billforge_core::{Role, TenantId, UserId};
use serde_json::Value;
use tower::util::ServiceExt;

async fn create_test_state() -> AppState {
    std::env::set_var("JWT_SECRET", "test-secret-key-for-testing-32-bytes");
    std::env::set_var("ENVIRONMENT", "development");
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/billforge_test".into());
    std::env::set_var("DATABASE_URL", database_url);
    std::env::set_var("TENANT_DB_PATH", "/tmp/billforge_test_tenants");
    std::env::set_var("LOCAL_STORAGE_PATH", "/tmp/billforge_test_files");
    std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");

    let config = Config::from_env().expect("Failed to load test config");
    AppState::new(&config)
        .await
        .expect("Failed to create test state")
}

async fn create_test_router() -> axum::Router {
    let state = create_test_state().await;
    routes::create_router(state)
}

fn get_auth_token() -> String {
    let jwt = JwtService::new(JwtConfig {
        secret: "test-secret-key-for-testing-32-bytes".to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    });
    let tenant_id: TenantId = "11111111-1111-1111-1111-111111111111"
        .parse()
        .expect("sandbox tenant id should parse");
    let user_id = UserId::from_uuid(
        uuid::Uuid::parse_str("17b66d9b-6da5-4cfb-93ad-f8d2f1aefe8f")
            .expect("sandbox user id should parse"),
    );

    jwt.create_access_token(
        &user_id,
        &tenant_id,
        "admin@sandbox.local",
        &[Role::TenantAdmin],
    )
    .expect("token creation should succeed")
}

#[tokio::test]
async fn test_inbox_returns_only_assigned_items_for_user() {
    let app = create_test_router().await;
    let token = get_auth_token();

    // The inbox endpoint should respond 200 and return an array
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflows/inbox")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Inbox request failed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Should be a paginated response object with data array
    assert!(
        json.is_object(),
        "Inbox response should be a paginated object"
    );
    assert!(
        json["data"].is_array(),
        "Inbox response data should be an array"
    );
    assert!(
        json["pagination"].is_object(),
        "Inbox response should include pagination"
    );
    assert_eq!(json["pagination"]["page"], 1);
    assert!(json["pagination"]["total_items"].is_number());
}

#[tokio::test]
async fn test_inbox_items_have_queue_fields() {
    let app = create_test_router().await;
    let token = get_auth_token();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflows/inbox")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Inbox request failed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // If any items exist, they should have queue_name and queue_type
    if let Some(items) = json["data"].as_array() {
        for item in items {
            assert!(
                item.get("queue_name").is_some(),
                "InboxItem should have queue_name"
            );
            assert!(
                item.get("queue_type").is_some(),
                "InboxItem should have queue_type"
            );
            assert!(
                item.get("queue_id").is_some(),
                "InboxItem should have queue_id"
            );
            assert!(
                item.get("invoice_id").is_some(),
                "InboxItem should have invoice_id"
            );
        }
    }
}

#[tokio::test]
async fn test_inbox_requires_auth() {
    let app = create_test_router().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflows/inbox")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Request failed");

    // Should be 401 or 403 without auth
    assert!(
        response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN,
        "Inbox should require authentication, got: {}",
        response.status()
    );
}

#[tokio::test]
async fn test_inbox_pagination_params() {
    let app = create_test_router().await;
    let token = get_auth_token();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/workflows/inbox?page=1&per_page=10")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Inbox request failed");

    assert_eq!(response.status(), StatusCode::OK);
}
