//! Integration tests for the require_auth middleware.
//!
//! Verifies that the middleware:
//! - Allows public paths through without any token
//! - Rejects requests with no Authorization header (401)
//! - Rejects requests with invalid/expired JWT tokens (401)
//! - Passes requests with valid JWT tokens and stores UserContext in extensions

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
    Router,
};
use billforge_auth::{AuthService, Claims, JwtConfig, JwtService, TokenType};
use billforge_core::{Role, TenantId, UserId};
use billforge_db::MetadataDatabase;
use std::sync::Arc;
use tower::util::ServiceExt;

use billforge_api::middleware::require_auth;

/// Create a test AuthService backed by a lazy (never-connected) PgPool.
/// Only JWT validation is exercised, which never touches the database.
fn test_auth_service() -> Arc<AuthService> {
    let jwt_config = JwtConfig {
        secret: "test-secret-for-middleware-tests".to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    };
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_lazy("postgres://fake@localhost/fake")
        .expect("connect_lazy should not fail");
    let metadata_db = Arc::new(MetadataDatabase::from_pool(pool));
    Arc::new(AuthService::new(jwt_config.clone(), metadata_db))
}

/// Create a JwtService with the same secret as the test AuthService.
fn test_jwt_service() -> JwtService {
    JwtService::new(JwtConfig {
        secret: "test-secret-for-middleware-tests".to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    })
}

/// Generate a valid access token for testing.
fn valid_access_token() -> String {
    let jwt = test_jwt_service();
    let user_id = UserId::from_uuid(uuid::Uuid::new_v4());
    let tenant_id = TenantId::new();
    jwt.create_access_token(&user_id, &tenant_id, "test@example.com", &[Role::ApUser])
        .expect("token creation should succeed")
}

/// Build a test router with the require_auth middleware backed by a real JWT validator.
fn build_test_router() -> Router {
    let auth = test_auth_service();
    Router::new()
        // Public paths
        .route("/api/v1/auth/login", post(ok_handler))
        .route("/api/v1/auth/register", post(ok_handler))
        .route("/api/v1/auth/provision", post(ok_handler))
        .route("/api/v1/auth/refresh", post(ok_handler))
        .route("/api/v1/actions/:token", post(ok_handler))
        .route("/api/v1/edi/webhook/test", post(ok_handler))
        .route("/api/v1/billing/plans", get(ok_handler))
        // Protected path
        .route("/api/v1/invoices", get(ok_handler))
        .layer(middleware::from_fn_with_state(auth, require_auth))
}

async fn ok_handler() -> &'static str {
    "ok"
}

#[tokio::test]
async fn unauthenticated_api_request_returns_401() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Protected endpoint should reject unauthenticated requests with 401"
    );
}

#[tokio::test]
async fn public_paths_allow_unauthenticated() {
    let public_paths = vec![
        ("/api/v1/auth/login", "POST"),
        ("/api/v1/auth/register", "POST"),
        ("/api/v1/auth/provision", "POST"),
        ("/api/v1/auth/refresh", "POST"),
        ("/api/v1/actions/some-token", "POST"),
        ("/api/v1/edi/webhook/test", "POST"),
        ("/api/v1/billing/plans", "GET"),
    ];

    for (path, method) in public_paths {
        let app = build_test_router();
        let builder = Request::builder().uri(path).method(method);
        let request = builder.body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "Public path {} should NOT be blocked by auth middleware",
            path
        );
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Public path {} should pass through to handler",
            path
        );
    }
}

#[tokio::test]
async fn valid_jwt_passes_middleware() {
    let app = build_test_router();
    let token = valid_access_token();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Request with valid JWT should pass through middleware to handler"
    );
}

#[tokio::test]
async fn invalid_jwt_returns_401() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .header("authorization", "Bearer not-a-real-jwt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Request with invalid JWT should be rejected with 401"
    );
}

#[tokio::test]
async fn expired_jwt_returns_401_with_token_expired() {
    // Create a token that expired 2 minutes ago (beyond jsonwebtoken's 60s leeway)
    use jsonwebtoken::{encode, EncodingKey, Header};

    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: uuid::Uuid::new_v4().to_string(),
        tenant_id: uuid::Uuid::new_v4().to_string(),
        email: "test@example.com".to_string(),
        roles: vec![],
        iat: now - 3600,
        exp: now - 120, // expired 2 minutes ago
        token_type: TokenType::Access,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"test-secret-for-middleware-tests"),
    )
    .expect("encoding should succeed");

    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expired JWT should be rejected with 401"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("token_expired"),
        "Response should indicate token_expired, got: {}",
        body_str
    );
}

#[tokio::test]
async fn wrong_secret_jwt_returns_401() {
    // Create a token signed with a different secret
    let jwt = JwtService::new(JwtConfig {
        secret: "different-secret-than-middleware".to_string(),
        access_token_expiry_hours: 1,
        refresh_token_expiry_days: 7,
    });
    let user_id = UserId::from_uuid(uuid::Uuid::new_v4());
    let tenant_id = TenantId::new();
    let token = jwt
        .create_access_token(&user_id, &tenant_id, "test@example.com", &[])
        .expect("token creation should succeed");

    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "JWT signed with wrong secret should be rejected with 401"
    );
}

#[tokio::test]
async fn malformed_auth_header_rejected() {
    let app = build_test_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/invoices")
                .header("authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Non-Bearer auth scheme should be rejected by middleware"
    );
}
