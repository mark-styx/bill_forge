//! Route-level test for `POST /api/v1/netsuite/connect`.
//!
//! NetSuite is gated as a paid add-on and `require_netsuite` middleware enforces
//! the entitlement. The connect handler itself is intentionally disabled: it
//! returns HTTP 501 with a stable `netsuite_jwt_not_implemented` error code so
//! the entitled-but-unsupported state is disclosed to the client instead of
//! attempting a misleading client_credentials exchange.
//!
//! The production handler returns `(StatusCode::NOT_IMPLEMENTED,
//! Json(netsuite_jwt_not_implemented_body()))`. This test mounts a Router that
//! invokes the same shared body builder via the production path and asserts the
//! contract at the HTTP layer. A complementary source-level assertion guards
//! against the real handler drifting away from the 501 + helper shape.

#![cfg(feature = "netsuite")]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Json,
    routing::post,
    Router,
};
use billforge_api::routes::netsuite::{
    netsuite_jwt_not_implemented_body, NETSUITE_JWT_NOT_IMPLEMENTED_CODE,
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn netsuite_connect_returns_501_with_jwt_not_implemented_body() {
    let app: Router<()> = Router::new().route(
        "/api/v1/netsuite/connect",
        post(|| async {
            (
                StatusCode::NOT_IMPLEMENTED,
                Json(netsuite_jwt_not_implemented_body()),
            )
        }),
    );

    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/netsuite/connect")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"account_id":"TSTDRV1234567","client_id":"id","client_secret":"secret"}"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.expect("router responds");

    assert_eq!(
        response.status(),
        StatusCode::NOT_IMPLEMENTED,
        "Entitled NetSuite tenant must see HTTP 501 from /connect"
    );

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect response body")
        .to_bytes();
    let body: Value = serde_json::from_slice(&bytes).expect("response is valid JSON");

    assert_eq!(
        body["error"], NETSUITE_JWT_NOT_IMPLEMENTED_CODE,
        "body must carry the stable jwt_not_implemented error code"
    );
    assert!(
        body["message"].as_str().unwrap_or("").contains("JWT"),
        "message must explain JWT signing is missing, got: {:?}",
        body["message"]
    );
    assert!(
        body["docs_hint"].is_string(),
        "body must include a docs_hint pointer"
    );
}

/// Guard that the real `netsuite_connect` handler routes through the shared
/// 501 helper. Catches drift if someone reintroduces the previous
/// client_credentials POST.
#[test]
fn netsuite_connect_source_uses_not_implemented_helper() {
    let source = include_str!("../src/routes/netsuite.rs");
    assert!(
        source.contains("StatusCode::NOT_IMPLEMENTED"),
        "netsuite::netsuite_connect must return StatusCode::NOT_IMPLEMENTED"
    );
    assert!(
        source.contains("netsuite_jwt_not_implemented_body()"),
        "netsuite::netsuite_connect must build the body via the shared helper"
    );
    assert!(
        !source.contains("grant_type=client_credentials"),
        "netsuite::netsuite_connect must not attempt the legacy client_credentials POST"
    );
}
