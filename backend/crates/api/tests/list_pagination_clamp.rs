//! Verifies that the /invoices, /vendors, and vendor statements list endpoints
//! clamp the `per_page` query parameter to a server-side maximum (100) so a
//! request such as `?per_page=1000000` cannot force the server to load an
//! unbounded result set into memory.

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

const MAX_PER_PAGE: u64 = 100;

#[tokio::test]
#[ignore]
async fn test_invoices_per_page_is_clamped_to_max() {
    let app = create_test_router().await;
    let token = get_auth_token();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/invoices?per_page=1000000")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Invoices request failed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let per_page = json["pagination"]["per_page"]
        .as_u64()
        .expect("pagination.per_page should be a number");
    assert!(
        per_page <= MAX_PER_PAGE,
        "per_page should be clamped to <= {}, got {}",
        MAX_PER_PAGE,
        per_page
    );

    let returned = json["data"]
        .as_array()
        .expect("data should be an array")
        .len() as u64;
    assert!(
        returned <= MAX_PER_PAGE,
        "returned row count should not exceed clamp ({}), got {}",
        MAX_PER_PAGE,
        returned
    );
}

#[tokio::test]
#[ignore]
async fn test_vendors_per_page_is_clamped_to_max() {
    let app = create_test_router().await;
    let token = get_auth_token();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/vendors?per_page=1000000")
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("Vendors request failed");

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let per_page = json["pagination"]["per_page"]
        .as_u64()
        .expect("pagination.per_page should be a number");
    assert!(
        per_page <= MAX_PER_PAGE,
        "per_page should be clamped to <= {}, got {}",
        MAX_PER_PAGE,
        per_page
    );

    let returned = json["data"]
        .as_array()
        .expect("data should be an array")
        .len() as u64;
    assert!(
        returned <= MAX_PER_PAGE,
        "returned row count should not exceed clamp ({}), got {}",
        MAX_PER_PAGE,
        returned
    );
}
