// src/tests.rs
use axum::{
    http::{Request, StatusCode},
    Router,
};
use serde_json::json;
use tower_test::mock::{MockLayer, MockService};

mod invoice_service;
mod database;

#[tokio::test]
async fn test_upload_invoice() {
    // Create a mock database service
    let mock_db = MockService::new_ok();
    let app = Router::new()
        .route("/invoices", post(upload_invoice))
        .layer(MockLayer::bind(mock_db.clone()));

    // Simulate an invoice upload request
    let req = Request::post("/invoices")
        .header("Content-Type", "multipart/form-data")
        .body(serde_json::to_vec(&json!({
            "file": "base64-encoded-file-content"
        })))
        .unwrap();

    // Send the request and receive the response
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Verify that the mock database was called with the correct data
    if let MockService::Result(_, Some(_)) = mock_db {
        // Add assertions to verify database interactions
    }
}

#[tokio::test]
async fn test_get_invoice_status() {
    // Create a mock database service
    let mock_db = MockService::new_ok();
    let app = Router::new()
        .route("/invoices/:id", get(get_invoice_status))
        .layer(MockLayer::bind(mock_db.clone()));

    // Simulate a request to get invoice status
    let req = Request::get("/invoices/123")
        .body(serde_json::to_vec(&json!({}))
        .unwrap();

    // Send the request and receive the response
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Verify that the mock database was called with the correct data
    if let MockService::Result(_, Some(_)) = mock_db {
        // Add assertions to verify database interactions
    }
}